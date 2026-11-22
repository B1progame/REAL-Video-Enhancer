from multiprocessing import shared_memory
import struct
import sys
import time
import numpy as np
import cv2

if __name__ != "__main__":
    from .utils.Util import log, padFrame, subprocess_popen_without_terminal
else:
    def log(message):
        print(message)




def convertTime(remaining_time):
    """
    Converts seconds to hours, minutes and seconds
    """
    hours = remaining_time // 3600
    remaining_time -= 3600 * hours
    minutes = remaining_time // 60
    remaining_time -= minutes * 60
    seconds = remaining_time
    if minutes < 10:
        minutes = str(f"0{minutes}")
    if seconds < 10:
        seconds = str(f"0{seconds}")
    return hours, minutes, seconds


class PauseManager:
    def __init__(self, paused_shared_memory_id):
        self.isPaused = False
        self.prevState = None
        self.paused_shared_memory_id = paused_shared_memory_id
        if self.paused_shared_memory_id is not None:
            while True:
                try:
                    self.pausedSharedMemory = shared_memory.SharedMemory(
                        name=self.paused_shared_memory_id
                    )
                    break
                except FileNotFoundError:
                    log(f"Waiting for shared memory to be created: {self.paused_shared_memory_id}")
                    time.sleep(0.5)
    def pause_manager(self):
        if self.paused_shared_memory_id is not None:
            return self.pausedSharedMemory.buf[0] == 1
        return False

class InformationWriteOut:
    def __init__(
        self,
        sharedMemoryID,  # image memory id
        sharedMemoryChunkSize,  # legacy raw frame size; shared preview uses a compact JPEG packet
        paused_shared_memory_id,
        outputWidth,
        outputHeight,
        croppedOutputWidth,
        croppedOutputHeight,
        totalOutputFrames,
        border_detect: bool = False,
        hdr_mode: bool = False,
    ):
        self.startTime = time.time()
        self.sharedMemoryID = sharedMemoryID
        self.paused_shared_memory_id = paused_shared_memory_id
        self.width = outputWidth
        self.height = outputHeight
        self.croppedOutputWidth = croppedOutputWidth
        self.croppedOututHeight = croppedOutputHeight
        self.totalOutputFrames = totalOutputFrames
        self.border_detect = border_detect
        self.previewFrame = None
        self.last_length = 0
        self.framesRendered = 0
        self.total_paused_time_seconds = 0
        self.hdr_mode = hdr_mode
        self.sharedMemoryChunkSize = sharedMemoryChunkSize

        if self.sharedMemoryID is not None:
            while True:
                try:
                    self.shm = shared_memory.SharedMemory(
                        name=self.sharedMemoryID
                    )
                    break
                except FileNotFoundError:
                    log(f"Waiting for shared memory to be created: {self.sharedMemoryID}")
                    time.sleep(0.5)
            
        self.pausedManager = PauseManager(paused_shared_memory_id)
        self.isPaused = False
        self.stop = False

    def realTimePrint(self, data):
        data = str(data)
        # Clear the last line
        sys.stdout.write("\r" + " " * self.last_length)
        sys.stdout.flush()

        # Write the new line
        sys.stdout.write("\r" + data)
        sys.stdout.flush()

        # Update the length of the last printed line
        self.last_length = len(data)

    def get_is_paused(self):
        return self.isPaused

    def calculateETA(self, framesRendered):
        """
        Calculates ETA

        Gets the time for every frame rendered by taking the
        elapsed time / completed iterations (files)
        remaining time = remaining iterations (files) * time per iteration

        """

        # Estimate the remaining time
        elapsed_time = time.time() - self.startTime
        time_per_iteration = elapsed_time / framesRendered
        remaining_iterations = self.totalOutputFrames - framesRendered
        remaining_time = remaining_iterations * time_per_iteration
        remaining_time = int(remaining_time)
        # convert to hours, minutes, and seconds
        hours, minutes, seconds = convertTime(remaining_time)
        return f"{hours}:{minutes}:{seconds}"

    def setPreviewFrame(self, frame):
        self.previewFrame = frame
        self._publishPreviewFrame(frame)

    def setFramesRendered(self, framesRendered: int):
        self.framesRendered = framesRendered

    def update(self, preview_frame):
        self.framesRendered += 1
        self.setPreviewFrame(preview_frame)

    def stopWriting(self):
        self.stop = True

    def writeOutInformation(self):
        """
        fcs = framechunksize
        """
        # Create a shared memory block
        if self.sharedMemoryID is not None:
            log(f"Shared memory name: {self.shm.name}")
        i = 0
        while not self.stop:
            time.sleep(0.5)  # setting this to a higher value will reduce the cpu usage, and increase fps
            self.isPaused = self.pausedManager.pause_manager()

            if self.isPaused:
                pause_start_time = time.time()
                while self.isPaused and not self.stop:
                    self.isPaused = self.pausedManager.pause_manager()
                    time.sleep(0.5)
                paused_duration = time.time() - pause_start_time
                self.total_paused_time_seconds += paused_duration

            if self.framesRendered > 0:
                # print out data to stdout
                active_time = time.time() - self.startTime - self.total_paused_time_seconds
                fps = self.framesRendered / max(active_time, 0.001)
                eta = self.calculateETA(framesRendered=self.framesRendered)
                message = f"FPS: {fps:.4f} Current Frame: {self.framesRendered} ETA: {eta}"
                if i == 0:
                    print("\n", file=sys.stderr)
                    i = 1
                self.realTimePrint(message)
    def _publishPreviewFrame(self, preview_frame):
        """Publish each completed output frame immediately for the browser."""
        if self.sharedMemoryID is None or preview_frame is None:
            return
        try:
            if self.border_detect:
                preview_frame = padFrame(
                    preview_frame,
                    self.width,
                    self.height,
                    self.croppedOutputWidth,
                    self.croppedOututHeight,
                )
            frame_data = bytes(preview_frame) if not isinstance(preview_frame, bytes) else preview_frame
            width, height = self.width, self.height
            dtype = np.uint16 if self.hdr_mode else np.uint8
            expected_bytes = width * height * 3 * np.dtype(dtype).itemsize
            if len(frame_data) < expected_bytes:
                raise ValueError(f"Preview frame is short ({len(frame_data)} < {expected_bytes} bytes)")
            frame = np.frombuffer(frame_data, dtype=dtype, count=width * height * 3).reshape(height, width, 3)
            if self.hdr_mode:
                frame = (np.clip(frame.astype(np.float32) / 65535.0, 0, 1) * 255).astype(np.uint8)
            if width > 1280:
                preview_height = max(1, round(height * 1280 / width))
                frame = cv2.resize(frame, (1280, preview_height), interpolation=cv2.INTER_AREA)
            else:
                preview_height = height
            # Frame objects store RGB; OpenCV's JPEG encoder expects BGR.
            frame = cv2.cvtColor(frame, cv2.COLOR_RGB2BGR)
            ok, encoded = cv2.imencode(".jpg", frame, [int(cv2.IMWRITE_JPEG_QUALITY), 78])
            if not ok:
                raise ValueError("JPEG encoder did not produce a preview frame")
            jpeg = encoded.tobytes()
            packet_capacity = len(self.shm.buf) - 16
            if len(jpeg) > packet_capacity:
                raise ValueError(f"Encoded preview exceeds shared buffer ({len(jpeg)} > {packet_capacity})")
            # Sequence lock: readers only accept a stable, even sequence.
            sequence = struct.unpack_from("<I", self.shm.buf, 0)[0]
            sequence = (sequence + 1) | 1
            struct.pack_into("<IIII", self.shm.buf, 0, sequence, 0, 0, 0)
            self.shm.buf[16:16 + len(jpeg)] = jpeg
            struct.pack_into("<IIII", self.shm.buf, 0, sequence + 1, min(width, 1280), preview_height, len(jpeg))
        except Exception as e:
            log("Failed to write preview frame to shared memory: " + str(e))
