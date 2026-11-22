from threading import Thread
import os
import math
import threading
import traceback
from time import sleep, time
from typing import Optional
import sys
from multiprocessing import shared_memory
import cv2

from .FFmpegBuffers import FFmpegRead, FFmpegWrite, MPVOutput
from .FFmpeg import InformationWriteOut
from .utils.Encoders import EncoderSettings
from .utils.SceneDetect import SceneDetect
from .utils.Util import log
from .utils.BorderDetect import BorderDetect
from .utils.VideoInfo import OpenCVInfo
from .utils.MotionBlur import WEIGHT_SCHEMES, blend_frame_samples
from .utils.FrameDeduplicate import FrameDuplicateDetector
import numpy as np


def global_thread_handler(args):
    # args.exc_value contains the error
    # args.exc_traceback contains the stack trace
    tb_string = ''.join(
        traceback.format_exception(args.exc_type, args.exc_value, args.exc_traceback)
    )
    print(f"Thread '{args.thread.name}' crashed. Full Traceback:\n{tb_string}", file=sys.stderr, flush=True)
    print('Exiting application due to thread crash.', file=sys.stderr, flush=True)
    sleep(1)  # Give time for the print to flush
    os._exit(1)


threading.excepthook = global_thread_handler


def remove_shared_memory_block(name):
    try:
        existing_shm = shared_memory.SharedMemory(name=name)
        existing_shm.close()
        existing_shm.unlink()
        print(f"Shared memory block '{name}' removed.")
    except FileNotFoundError:
        print(f"Shared memory block '{name}' does not exist.")
    except Exception as e:
        print(f"Error removing shared memory block '{name}': {e}")


class Render:
    """
    Subclass of FFmpegRender
    FFMpegRender options:
    inputFile: str, The path to the input file.
    outputFile: str, The path to the output file.
    interpolateTimes: int, this sets the multiplier for the framerate when interpolating, when only upscaling this will be set to 1.
    encoder: str, The exact name of the encoder ffmpeg will use (default=libx264)
    pixelFormat: str, The pixel format ffmpeg will use, (default=yuv420p)

    interpolateOptions:
    interpolationMethod
    upscaleModel
    backend (pytorch,ncnn,tensorrt)
    device (cpu,cuda)
    precision (float16,float32)

    NOTE:
    Everything in here has to happen in a specific order:
    Get the video properties (res,fps,etc)
    set up upscaling/interpolation, this gets the scale for upscaling if upscaling is the current task
    assign framechunksize to a value, as this is needed to catch bytes and set up shared memory
    set up shared memory
    """

    def __init__(
        self,
        inputFile: str,
        outputFile: str,
        # backend settings
        backend="pytorch",
        device="default",
        precision="float16",
        pytorch_gpu_id: int = 0,
        ncnn_gpu_id: int = 0,
        cwd: str = os.getcwd(),
        # model settings
        upscaleModel=None,
        interpolateModel=None,
        interpolateFactor: float = 1.0,
        extraRestorationModels=None,
        denoise_model_index: int = -1,
        denoise_strength: int = 10,
        denoise_temporal: str = "off",
        denoise_keep_film_grain: bool = False,
        deblur_model_index: int = -1,
        deblur_strength: int = 10,
        deblur_edge_recovery: int = 0,
        deblur_ghosting_guard: str = "off",
        sceneDetectModel: str = None,
        tile_size=None,
        drba=False,
        # ffmpeg settings
        ffmpeg_path: str = "./bin/ffmpeg",
        start_time=None,
        end_time=None,
        custom_encoder: str = "libx264",
        pixelFormat: str = "yuv420p",
        benchmark: bool = False,
        overwrite: bool = False,
        crf: str = "18",
        video_encoder_preset: str = "libx264",
        audio_encoder_preset: str = "aac",
        subtitle_encoder_preset: str = "srt",
        audio_bitrate: str = "192k",
        border_detect: bool = False,
        hdr_mode: bool = False,
        merge_subtitles: bool = True,
        # misc
        pause_shared_memory_id=None,
        sceneDetectMethod: str = "pyscenedetect",
        sceneDetectSensitivity: float = 3.0,
        sharedMemoryID: Optional[str] = None,
        trt_optimization_level: int = 3,
        trt_dynamic_shapes: bool = False,
        override_upscale_scale: int | None = None,
        UHD_mode: bool = False,
        slomo_mode: bool = False,
        dynamic_scaled_optical_flow: bool = False,
        ensemble: bool = False,
        output_to_mpv: bool = False,
        motion_blur: bool = False,
        motion_blur_shutter_angle: float = 180.0,
        motion_blur_strength: str = "gaussian_sym",
        motion_blur_gamma: bool = True,
        deduplicate: bool = False,
        deduplicate_threshold: float = 0.9,
    ):
        self.inputFile = inputFile
        self.backend = backend
        self.upscaleModel = upscaleModel
        self.interpolateModel = interpolateModel
        self.tilesize = tile_size
        self.device = device
        self.precision = precision
        self.interpolateFactor = interpolateFactor
        self.motion_blur = motion_blur
        self.motion_blur_shutter_angle = motion_blur_shutter_angle
        self.motion_blur_strength = motion_blur_strength
        self.motion_blur_gamma = motion_blur_gamma
        self.deduplicate = bool(deduplicate)
        self.deduplicate_threshold = float(deduplicate_threshold)
        if self.deduplicate and (not self.interpolateModel or self.interpolateFactor <= 1):
            raise ValueError("Duplicate-frame skipping requires interpolation with a factor greater than 1")
        self.duplicate_detector = (
            FrameDuplicateDetector(threshold=self.deduplicate_threshold)
            if self.deduplicate
            else None
        )
        if self.motion_blur:
            if slomo_mode:
                raise ValueError("Motion blur cannot be combined with slow-motion output mode")
            if not self.interpolateModel or self.interpolateFactor <= 1:
                raise ValueError("Motion blur requires interpolation with a factor greater than 1")
            if not 0.0 <= float(self.motion_blur_shutter_angle) <= 360.0:
                raise ValueError("Motion-blur shutter angle must be between 0 and 360 degrees")
            if self.motion_blur_strength not in WEIGHT_SCHEMES:
                raise ValueError(f"Unknown motion-blur strength: {self.motion_blur_strength}")
        # max timestep is a hack to make sure ncnn cache frames too early, and ncnn breaks if i modify the code at all so ig this is what we are doing
        # also used to help with performace and caching
        # must use ceilInterpolateFactor so the last timestep matches exactly for decimal factors
        self.ceilInterpolateFactor = math.ceil(self.interpolateFactor)
        self.maxTimestep = (self.ceilInterpolateFactor - 1) / self.ceilInterpolateFactor
        
        # self.setupRender = self.returnFrame  # set it to not convert the bytes to array by default, and just pass chunk through
        self.setupFrame0 = None
        self.interpolateOption = None
        self.upscaleOption = None
        self.isPaused = False
        self.drba = drba
        self.sceneDetectModelPath = sceneDetectModel
        self.sceneDetectMethod = sceneDetectMethod
        self.sceneDetectSensitivty = sceneDetectSensitivity
        self.sharedMemoryID = sharedMemoryID
        self.trt_optimization_level = trt_optimization_level
        self.uncacheNextFrame = False
        self.UHD_mode = UHD_mode
        self.dynamic_scaled_optical_flow = dynamic_scaled_optical_flow
        self.ensemble = ensemble
        self.pytorch_gpu_id = pytorch_gpu_id
        self.ncnn_gpu_id = ncnn_gpu_id
        self.outputFrameChunkSize = None
        self.hdr_mode = hdr_mode
        self.override_upscale_scale = override_upscale_scale
        self.trt_dynamic_shapes = trt_dynamic_shapes
        self.extraRestorationModels = []
        self.denoise_model_index = int(denoise_model_index)
        self.denoise_strength = min(10, max(1, int(denoise_strength)))
        self.denoise_temporal = str(denoise_temporal or "off").lower()
        self.denoise_keep_film_grain = bool(denoise_keep_film_grain)
        self.deblur_model_index = int(deblur_model_index)
        self.deblur_strength = min(10, max(1, int(deblur_strength)))
        self.deblur_edge_recovery = min(100, max(0, int(deblur_edge_recovery)))
        self.deblur_ghosting_guard = str(deblur_ghosting_guard or "off").lower()
        self._previous_denoise_gray = None
        self._previous_denoised_frame = None
        
        if cwd:
            log("Working Directory: " + cwd)
        else:
            cwd = os.getcwd()
            log("No Working Directory specified, using current directory: " + cwd)
        videoInfo = OpenCVInfo(input_file=inputFile, start_time=start_time, end_time=end_time, ffmpeg_path=ffmpeg_path)
        
        if not videoInfo.is_valid_video:
            log("Input video is not valid!")
        
        if start_time is None:
            start_time = 0
        if end_time is None:
            end_time = videoInfo.get_duration_seconds()

        self.width, self.height = videoInfo.get_width_x_height()
        self.originalWidth = self.width
        self.originalHeight = self.height
        input_pix_fmt = videoInfo.get_pixel_format()
        self.borderX = 0
        self.borderY = 0  # set borders for cropping automatically to 0, will be overwritten if borders are detected
        self.totalInputFrames = videoInfo.get_total_frames()
        self.totalOutputFrames = (
            self.totalInputFrames
            if self.motion_blur
            else int(self.totalInputFrames * self.ceilInterpolateFactor)
        )
        self.fps = videoInfo.get_fps()
        color_space = videoInfo.get_color_space()
        color_primaries = videoInfo.get_color_primaries()
        color_transfer = videoInfo.get_color_transfer()

        video_encoder = EncoderSettings(video_encoder_preset)
        audio_encoder = EncoderSettings(audio_encoder_preset, type="audio")
        subtitle_encoder = EncoderSettings(subtitle_encoder_preset, type="subtitle")

        if border_detect:  # border detect has to be put before everything, to overwrite the width and height
            print("Detecting borders", file=sys.stderr)
            borderDetect = BorderDetect(inputFile=self.inputFile, ffmpeg_path=ffmpeg_path)
            self.width, self.height, self.borderX, self.borderY = (
                borderDetect.getBorders()
            )
            log(
                f"Detected borders: Width,Height:{self.width}x{self.height}, X,Y: {self.borderX}x{self.borderY}"
            )

        log("Using backend: " + self.backend)
        # upscale has to be called first to get the scale of the upscale model
        if upscaleModel:
            self.setupUpscale()
            self.upscaleOption.hotUnload()  # unload model to free up memory for trt enging building
            log("Using Upscaling Model: " + self.upscaleModel)
        else:
            self.upscaleTimes = 1  # if no upscaling, it will default to 1
            self.modelScale = 1
            

        
        if extraRestorationModels:
            for model in extraRestorationModels:
                extraRestoration = self.setupExtraRestoration(model)
                if extraRestoration:
                    log("Using Extra Restoration Model: " + model)
                    self.extraRestorationModels.append(extraRestoration)
                    extraRestoration.hotUnload()  # unload model to free up memory for trt enging building
        

        if interpolateModel:
            self.setupInterpolate()
            log("Using Interpolation Model: " + self.interpolateModel)

        if upscaleModel: # load model after interpolation model is loaded, this saves on vram if the user builds 2 separate engines
            self.upscaleOption.hotReload()

        for extraRestoration in self.extraRestorationModels:
            extraRestoration.hotReload()
        
        if self.modelScale and self.override_upscale_scale:
            if int(self.modelScale) == int(self.override_upscale_scale):
                log("Override upscale scale is set to the same value as the model scale, this will not change the output resolution.")
                self.override_upscale_scale = False
        
        log(f"Upscale Times: {self.override_upscale_scale if self.override_upscale_scale else self.upscaleTimes}")
        log(f"Interpolate Factor: {self.interpolateFactor}")
        log(f"Total Output Frames: {self.totalOutputFrames}")
        log("Model Scale: " + str(self.modelScale))
        log("HDR Mode: " + str(hdr_mode))

        self.readBuffer = FFmpegRead(  # input width
            inputFile=inputFile,
            width=self.width,
            height=self.height,
            start_time=start_time,
            end_time=end_time,
            borderX=self.borderX,
            borderY=self.borderY,
            hdr_mode=hdr_mode,
            backend=self.backend,
            device=self.device,
            gpu_id=self.pytorch_gpu_id if self.backend in ["pytorch","tensorrt"] else self.ncnn_gpu_id,
            dtype=self.precision,
            color_space=color_space,
            color_primaries=color_primaries,
            color_transfer=color_transfer,
            input_pixel_format=input_pix_fmt,
            ffmpeg_path=ffmpeg_path,
        )

        self.writeBuffer = FFmpegWrite(
            inputFile=inputFile,
            outputFile=outputFile,
            width=self.width,
            height=self.height,
            start_time=start_time,
            end_time=end_time,
            fps=self.fps,
            crf=crf,
            audio_bitrate=audio_bitrate,
            pixelFormat=pixelFormat,
            overwrite=overwrite,
            custom_encoder=custom_encoder,
            benchmark=benchmark,
            slowmo_mode=slomo_mode,
            upscaleTimes=self.upscaleTimes if not self.override_upscale_scale else self.override_upscale_scale,
            interpolateFactor=1.0 if self.motion_blur else self.interpolateFactor,
            ceilInterpolateFactor=1 if self.motion_blur else self.ceilInterpolateFactor,
            video_encoder=video_encoder,
            audio_encoder=audio_encoder,
            subtitle_encoder=subtitle_encoder,
            mpv_output=output_to_mpv,
            hdr_mode=hdr_mode,
            merge_subtitles=merge_subtitles,
            color_space=color_space,
            color_primaries=color_primaries,
            color_transfer=color_transfer,
            ffmpeg_path=ffmpeg_path,
            ffmpeg_log_file=os.path.join(cwd, "ffmpeg_log.txt"),
        )

        shm_mul = self.override_upscale_scale if self.override_upscale_scale else self.upscaleTimes
        hdr_mul = 6 if hdr_mode else 3

        self.informationHandler = InformationWriteOut(
            sharedMemoryID=sharedMemoryID,
            sharedMemoryChunkSize=self.originalHeight*self.originalWidth*shm_mul*shm_mul*hdr_mul,
            paused_shared_memory_id=pause_shared_memory_id,
            outputWidth=self.originalWidth*shm_mul,
            outputHeight=self.originalHeight*shm_mul,
            croppedOutputWidth=self.width*shm_mul,
            croppedOutputHeight=self.height*shm_mul,
            totalOutputFrames=self.totalOutputFrames,
            border_detect=border_detect,
            hdr_mode=hdr_mode,
            
        )
        

        self.renderThread = Thread(target=self.render)
        self.ffmpegReadThread = Thread(target=self.readBuffer.read_frames_into_queue)
        self.ffmpegWriteThread = Thread(target=self.writeBuffer.write_out_frames)
        self.sharedMemoryThread = Thread(
            target=self.informationHandler.writeOutInformation
        )

        # PyTorch registers process-wide shutdown hooks during import. Import
        # and initialize its frame utilities on the main thread before worker
        # threads start, otherwise the first decoded frame can import torch
        # after Python has begun interpreter shutdown.
        if self.backend in ("pytorch", "tensorrt"):
            from .utils.Frame import _init_pytorch

            _init_pytorch(
                self.device,
                self.pytorch_gpu_id,
                self.precision,
                self.width,
                self.height,
                self.hdr_mode,
                backend=self.backend,
            )

        self.sharedMemoryThread.start()
        self.ffmpegReadThread.start()
        self.ffmpegWriteThread.start()
        self.renderThread.start()

        if output_to_mpv:
            output_fps = self.fps if self.motion_blur else self.fps*self.interpolateFactor
            MPVOut = MPVOutput(self.writeBuffer, width=self.width*self.upscaleTimes, height=self.height*self.upscaleTimes,fps=output_fps, outputFrameChunkSize=self.outputFrameChunkSize)
            MPVoutThread = Thread(target=MPVOut.write_out_frames)
            MPVoutThread.start()

    def write_bytes_to_cv2_frame_debug(self, frame):
        # Convert the byte array to a numpy array
        frame_array = np.frombuffer(frame, dtype=np.uint8)
        # Reshape the array to the correct dimensions
        frame_array = frame_array.reshape((self.height, self.width, 3))
        # Convert the BGR image to RGB
        frame_array = cv2.cvtColor(frame_array, cv2.COLOR_BGR2RGB)
        cv2.imwrite("frame.jpg", frame_array)

    def render(self):
        frames_rendered = 0
        previous_motion_frame = None

        def write_frame(frame):
            nonlocal frames_rendered
            if self.upscaleModel:
                frame = self.upscaleOption(frame)
            if self.override_upscale_scale:
                frame = frame.resize_frame(
                    self.width * self.override_upscale_scale,
                    self.height * self.override_upscale_scale,
                )

            bytes_out = frame.get_frame_bytes() if type(frame) != bytes else frame
            self.informationHandler.update(bytes_out)
            self.writeBuffer.writeQueue.put(bytes_out)
            frames_rendered += 1

        """
        from viztracer import VizTracer
        tracer = VizTracer()
        tracer.start()
        from pyinstrument import Profiler
        profiler = Profiler()
        profiler.start()
        """

        while True:
            if self.informationHandler.get_is_paused():
                sleep(1)
                continue

            frame = self.readBuffer.get()
            #self.write_bytes_to_cv2_frame_debug(frame)
            if frame is None:
                if self.motion_blur and previous_motion_frame is not None:
                    write_frame(previous_motion_frame)
                self.informationHandler.stopWriting()
                break

            scene_change = False
            if self.interpolateModel:  # detect scene changes before any calculations, as cv2 misbehaves on AI-processed frames
                scene_change = self.sceneDetect.detect(frame)

            duplicate_detector = getattr(self, "duplicate_detector", None)
            is_duplicate = duplicate_detector.is_duplicate(frame) if duplicate_detector else False
            # Interpolators already treat transitions as a request to emit
            # copies of the current frame and advance their internal reference
            # without running optical-flow inference. This preserves CFR timing.
            skip_interpolation = scene_change or is_duplicate

            for restoration_index, extraRestoration in enumerate(self.extraRestorationModels):
                is_denoise = restoration_index == self.denoise_model_index
                is_deblur = restoration_index == self.deblur_model_index
                source_frame = frame if is_denoise or is_deblur else None
                frame = extraRestoration(frame)
                if source_frame is not None:
                    if is_denoise:
                        frame = self.processDenoiseFrame(source_frame, frame)
                    elif is_deblur:
                        frame = self.processDeblurFrame(source_frame, frame)

            if self.motion_blur:
                interpolated_frames = list(
                    self.interpolateOption(img1=frame, transition=skip_interpolation)
                )
                if previous_motion_frame is None:
                    previous_motion_frame = frame.clone()
                    continue

                if scene_change or not interpolated_frames:
                    output_frame = frame
                else:
                    output_frame = blend_frame_samples(
                        [previous_motion_frame, *interpolated_frames, frame],
                        shutter_angle=self.motion_blur_shutter_angle,
                        strength=self.motion_blur_strength,
                        gamma_correct=self.motion_blur_gamma,
                    )
                previous_motion_frame = frame.clone()
                write_frame(output_frame)
                continue

            if self.interpolateModel:
                for interpolated_frame in self.interpolateOption(
                    img1=frame,
                    transition=skip_interpolation,
                ):
                    write_frame(interpolated_frame)

            write_frame(frame)
        self.writeBuffer.writeQueue.put(None)
        """
        tracer.stop()
        tracer.save()
        profiler.stop()
        print(profiler.output_text(unicode=True, color=True))
        """
    
    def upscalePytorchObject(self, modelPath=None):
        from .pytorch.UpscaleTorch import UpscalePytorch
        return UpscalePytorch(
            modelPath,
            device=self.device,
            precision=self.precision,
            width=self.width,
            height=self.height,
            backend=self.backend,
            tilesize=self.tilesize,
            gpu_id=self.pytorch_gpu_id,
            trt_optimization_level=self.trt_optimization_level,
            hdr_mode=self.hdr_mode,
            trt_static_shape= not self.trt_dynamic_shapes,
        )
    
    def upscaleNCNNObject(self, scale=None, modelPath=None):
        from .ncnn.UpscaleNCNN import UpscaleNCNN
        path, last_folder = os.path.split(modelPath)
        modelPath = os.path.join(path, last_folder, last_folder)
        return UpscaleNCNN(
            modelPath=modelPath,
            num_threads=1,
            scale=self.upscaleTimes if scale is None else scale,
            gpuid=self.ncnn_gpu_id,  # might have this be a setting
            width=self.width,
            height=self.height,
            tilesize=self.tilesize,
        )
    def upscaleONNXObject(self, scale=None, modelPath=None):
        from .onnx.UpscaleONNX import UpscaleONNX
        return UpscaleONNX(
            modelPath=modelPath,
            deviceID=self.pytorch_gpu_id,
            precision=self.precision,
            width=self.width,
            height=self.height,
            scale=self.upscaleTimes if scale is None else scale,
            hdr_mode=self.hdr_mode,
        )


    def setupExtraRestoration(self, modelPath):
        log("Setting up Extra Restoration")
        if self.backend == "pytorch" or self.backend == "tensorrt":
            return self.upscalePytorchObject(modelPath)
        
        if self.backend == "ncnn":
            return self.upscaleNCNNObject(scale=1, modelPath=modelPath)

    def blendRestorationFrame(self, source, restored, strength):
        """Blend a restoration result with its input, keeping tensor work on GPU."""
        if strength >= 1.0:
            return restored
        if self.backend in ("pytorch", "tensorrt"):
            original_tensor = source.get_frame_tensor()
            restored_tensor = restored.get_frame_tensor()
            return restored.set_frame_tensor(original_tensor.lerp(restored_tensor, strength))
        original_np = source.get_frame_np()
        restored_np = restored.get_frame_np()
        return restored.set_frame_np(
            cv2.addWeighted(original_np, 1.0 - strength, restored_np, strength, 0)
        )

    @staticmethod
    def _frame_max_value(array):
        return float(np.iinfo(array.dtype).max) if np.issubdtype(array.dtype, np.integer) else 1.0

    def processDenoiseFrame(self, source, denoised):
        denoised = self.blendRestorationFrame(source, denoised, self.denoise_strength / 10.0)
        if self.denoise_temporal in ("light", "medium", "strong"):
            source_sdr = source.get_np_sdr()
            current_gray = cv2.cvtColor(source_sdr, cv2.COLOR_BGR2GRAY)
            current = denoised.get_frame_np().astype(np.float32)
            if self._previous_denoise_gray is not None and self._previous_denoised_frame is not None:
                change = float(cv2.absdiff(current_gray, self._previous_denoise_gray).mean())
                if change < 28.0 and self._previous_denoised_frame.shape == current.shape:
                    height, width = current_gray.shape[:2]
                    flow_scale = min(1.0, 640.0 / max(width, height))
                    flow_size = (max(1, round(width * flow_scale)), max(1, round(height * flow_scale)))
                    flow_current = cv2.resize(current_gray, flow_size, interpolation=cv2.INTER_AREA)
                    flow_previous = cv2.resize(self._previous_denoise_gray, flow_size, interpolation=cv2.INTER_AREA)
                    flow_small = cv2.calcOpticalFlowFarneback(flow_current, flow_previous, None,
                                                              0.5, 3, 15, 3, 5, 1.2, 0)
                    flow = cv2.resize(flow_small, (width, height), interpolation=cv2.INTER_LINEAR)
                    flow[..., 0] *= width / flow_size[0]
                    flow[..., 1] *= height / flow_size[1]
                    y, x = np.mgrid[:height, :width].astype(np.float32)
                    warped = cv2.remap(self._previous_denoised_frame,
                                       x + flow[..., 0], y + flow[..., 1],
                                       cv2.INTER_LINEAR, borderMode=cv2.BORDER_REPLICATE)
                    amount = {"light": 0.12, "medium": 0.22, "strong": 0.32}[self.denoise_temporal]
                    current = current * (1.0 - amount) + warped * amount
            self._previous_denoise_gray = current_gray.copy()
            self._previous_denoised_frame = current.copy()
            denoised = denoised.set_frame_np(np.clip(current, 0, self._frame_max_value(denoised.get_frame_np())).astype(denoised.get_frame_np().dtype))

        if self.denoise_keep_film_grain:
            original = source.get_frame_np().astype(np.float32)
            output = denoised.get_frame_np().astype(np.float32)
            low_frequency = cv2.GaussianBlur(original, (0, 0), 1.0)
            preserved_texture = (original - low_frequency) * 0.35
            output += preserved_texture
            source_dtype = denoised.get_frame_np().dtype
            denoised = denoised.set_frame_np(np.clip(output, 0, self._frame_max_value(denoised.get_frame_np())).astype(source_dtype))
        return denoised

    def processDeblurFrame(self, source, deblurred):
        guard_factor = {"off": 1.0, "conservative": 0.82, "aggressive": 0.62}.get(self.deblur_ghosting_guard, 0.82)
        strength = (self.deblur_strength / 10.0) * guard_factor
        deblurred = self.blendRestorationFrame(source, deblurred, strength)
        if self.deblur_edge_recovery <= 0:
            return deblurred
        restored = deblurred.get_frame_np().astype(np.float32)
        blur = cv2.GaussianBlur(restored, (0, 0), 0.8)
        amount = (self.deblur_edge_recovery / 100.0) * 0.7
        restored += (restored - blur) * amount
        dtype = deblurred.get_frame_np().dtype
        return deblurred.set_frame_np(np.clip(restored, 0, self._frame_max_value(deblurred.get_frame_np())).astype(dtype))

    def setupUpscale(self):
        log("Setting up Upscale")
        if self.backend == "pytorch" or self.backend == "tensorrt":

            self.upscaleOption = self.upscalePytorchObject(self.upscaleModel)
            self.modelScale = self.upscaleOption.getScale() 
            

        if self.backend == "ncnn":
            from .ncnn.UpscaleNCNN import getNCNNScale

            self.modelScale = getNCNNScale(modelPath=self.upscaleModel)
            
            self.upscaleOption = self.upscaleNCNNObject(scale=self.modelScale, modelPath=self.upscaleModel)

        if self.backend == "directml":  # i dont want to work with this shit
            from .onnx.UpscaleONNX import UpscaleONNX
            self.modelScale = UpscaleONNX.getModelScale(self.upscaleModel)
            
            self.upscaleOption = UpscaleONNX(
                modelPath=self.upscaleModel,
                precision=self.precision,
                width=self.width,
                height=self.height,
                scale=self.modelScale
            )
        self.upscaleTimes = self.modelScale if not self.override_upscale_scale else self.override_upscale_scale
        


    def setupInterpolate(self):
        log("Setting up Interpolation")
        self.sceneDetect = SceneDetect(
            sceneChangeMethod=self.sceneDetectMethod,
            sceneChangeSensitivity=self.sceneDetectSensitivty,
            width=self.width,
            height=self.height,
            model_path=self.sceneDetectModelPath,
            model_backend=self.backend,
            model_dtype=self.precision,
            model_device=self.device,
            model_gpu_id=self.pytorch_gpu_id if self.backend in ["pytorch","tensorrt"] else self.ncnn_gpu_id,
        )
        if self.sceneDetectMethod != "none":
            log("Scene Detection Enabled")

        else:
            log("Scene Detection Disabled")

        if self.backend == "ncnn":
            from .ncnn.InterpolateNCNN import InterpolateRIFENCNN

            self.interpolateOption = InterpolateRIFENCNN(
                interpolateModelPath=self.interpolateModel,
                width=self.width,
                height=self.height,
                gpuid=self.ncnn_gpu_id,
                max_timestep=self.maxTimestep,
                interpolateFactor=self.ceilInterpolateFactor,
                hdr_mode=self.hdr_mode,
            )

        if self.backend == "pytorch" or self.backend == "tensorrt":
            from .pytorch.InterpolateTorch import InterpolateFactory

            self.interpolateOption = InterpolateFactory.build_interpolation_method(
                self.interpolateModel,
                self.backend,
                self.drba,
            )(
                modelPath=self.interpolateModel,
                ceilInterpolateFactor=self.ceilInterpolateFactor,
                width=self.width,
                height=self.height,
                device=self.device,
                dtype=self.precision,
                backend=self.backend,
                gpu_id=self.pytorch_gpu_id,
                UHDMode=self.UHD_mode,
                drba=self.drba,
                trt_optimization_level=self.trt_optimization_level,
                ensemble=self.ensemble,
                dynamicScaledOpticalFlow=self.dynamic_scaled_optical_flow,
                max_timestep=self.maxTimestep,
                hdr_mode=self.hdr_mode,
                trt_static_shape= not self.trt_dynamic_shapes,  # if dynamic shapes are enabled, we have to set the static shape to false (default is true in the model
            )
