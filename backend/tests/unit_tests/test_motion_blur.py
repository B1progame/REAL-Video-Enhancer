import os
import sys

import numpy as np
import pytest


BACKEND_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
if BACKEND_DIR not in sys.path:
    sys.path.insert(0, BACKEND_DIR)

from src.utils.MotionBlur import blend_arrays, generate_weights, select_shutter_samples
from src.utils.FrameDeduplicate import FrameDuplicateDetector
from src.utils.Frame import Frame


@pytest.mark.parametrize("scheme", ["gaussian_sym", "equal", "pyramid", "ascending", "descending"])
def test_weight_schemes_are_normalized_and_centered(scheme):
    weights = generate_weights(5, scheme)
    assert len(weights) == 5
    assert sum(weights) == pytest.approx(1.0)
    if scheme in ("gaussian_sym", "pyramid"):
        assert weights[2] == max(weights)
    elif scheme == "equal":
        assert len(set(weights)) == 1
    elif scheme == "ascending":
        assert weights[-1] == max(weights)
    else:
        assert weights[0] == max(weights)


def test_equal_weight_sdr_blend_uses_linear_light_by_default():
    black = np.zeros((2, 2, 3), dtype=np.uint8)
    white = np.full((2, 2, 3), 255, dtype=np.uint8)

    linear_light = blend_arrays([black, white], [1, 1])
    encoded_space = blend_arrays([black, white], [1, 1], gamma_correct=False)

    assert np.all(linear_light == 186)
    assert np.all(encoded_space == 128)


def test_hdr_blend_preserves_uint16_range():
    dark = np.zeros((1, 1, 3), dtype=np.uint16)
    bright = np.full((1, 1, 3), 65535, dtype=np.uint16)

    blended = blend_arrays([dark, bright], [1, 1])

    assert blended.dtype == np.uint16
    assert np.all(blended == 32768)


def test_shutter_window_ends_at_output_timestamp():
    frames = list(range(5))
    assert select_shutter_samples(frames, 0) == [4]
    assert select_shutter_samples(frames, 180) == [2, 3, 4]
    assert select_shutter_samples(frames, 360) == frames


def test_bad_motion_blur_inputs_fail_clearly():
    with pytest.raises(ValueError, match="Unknown"):
        generate_weights(3, "random")
    with pytest.raises(ValueError, match="same shape"):
        blend_arrays([np.zeros((2, 2), dtype=np.uint8), np.zeros((1, 2), dtype=np.uint8)])


def test_render_motion_blur_emits_one_frame_per_input():
    from src.RenderVideo import Render

    class Reader:
        def __init__(self, frames):
            self.frames = iter([*frames, None])

        def get(self):
            return next(self.frames)

    class Writer:
        def __init__(self):
            self.frames = []
            self.writeQueue = self

        def put(self, frame):
            self.frames.append(frame)

    class Information:
        def __init__(self):
            self.frames = []
            self.stopped = False

        @staticmethod
        def get_is_paused():
            return False

        def update(self, frame):
            self.frames.append(frame)

        def stopWriting(self):
            self.stopped = True

    class Interpolator:
        def __init__(self):
            self.previous = None

        def __call__(self, img1, transition=False):
            if self.previous is None:
                self.previous = img1.clone()
                return []
            midpoint = np.rint(
                (self.previous.get_frame_np().astype(np.float32) + img1.get_frame_np()) / 2
            ).astype(np.uint8)
            self.previous = img1.clone()
            middle_frame = img1.get_dummy_frame().set_frame_np(midpoint)
            return [middle_frame]

    render = Render.__new__(Render)
    render.motion_blur = True
    render.motion_blur_shutter_angle = 360
    render.motion_blur_strength = "equal"
    render.motion_blur_gamma = False
    render.interpolateModel = "fake-rife"
    render.interpolateOption = Interpolator()
    render.sceneDetect = type("SceneDetector", (), {"detect": lambda self, frame: False})()
    render.extraRestorationModels = []
    render.upscaleModel = None
    render.override_upscale_scale = None
    render.width = 1
    render.height = 1
    render.readBuffer = Reader(
        [
            Frame("onnx", 1, 1, "cpu", 0, False, "float32").set_frame_np(
                np.zeros((1, 1, 3), dtype=np.uint8)
            ),
            Frame("onnx", 1, 1, "cpu", 0, False, "float32").set_frame_np(
                np.full((1, 1, 3), 255, dtype=np.uint8)
            ),
        ]
    )
    render.writeBuffer = Writer()
    render.informationHandler = Information()

    render.render()

    output = render.writeBuffer.frames
    assert len(output) == 3  # two encoded frames plus the queue terminator
    assert np.frombuffer(output[0], dtype=np.uint8).tolist() == [128, 128, 128]
    assert np.frombuffer(output[1], dtype=np.uint8).tolist() == [255, 255, 255]
    assert output[2] is None
    assert render.informationHandler.stopped


def test_motion_blur_and_deduplication_preserve_cfr_frame_count():
    from src.RenderVideo import Render

    class Reader:
        def __init__(self):
            self.frames = iter(
                [
                    Frame("onnx", 1, 1, "cpu", 0, False, "float32").set_frame_np(
                        np.zeros((1, 1, 3), dtype=np.uint8)
                    ),
                    Frame("onnx", 1, 1, "cpu", 0, False, "float32").set_frame_np(
                        np.zeros((1, 1, 3), dtype=np.uint8)
                    ),
                    Frame("onnx", 1, 1, "cpu", 0, False, "float32").set_frame_np(
                        np.full((1, 1, 3), 255, dtype=np.uint8)
                    ),
                    None,
                ]
            )

        def get(self):
            return next(self.frames)

    class Writer:
        def __init__(self):
            self.frames = []
            self.writeQueue = self

        def put(self, frame):
            self.frames.append(frame)

    class Information:
        def __init__(self):
            self.frames = []
            self.stopped = False

        @staticmethod
        def get_is_paused():
            return False

        def update(self, frame):
            self.frames.append(frame)

        def stopWriting(self):
            self.stopped = True

    class Interpolator:
        def __init__(self):
            self.previous = None
            self.transitions = []

        def __call__(self, img1, transition=False):
            self.transitions.append(transition)
            if self.previous is None:
                self.previous = img1.clone()
                return []
            if transition:
                self.previous = img1.clone()
                return [img1]
            middle = np.rint(
                (img1.get_frame_np().astype(np.float32) + self.previous.get_frame_np())
                / 2
            ).astype(np.uint8)
            self.previous = img1.clone()
            return [img1.get_dummy_frame().set_frame_np(middle)]

    render = Render.__new__(Render)
    render.motion_blur = True
    render.motion_blur_shutter_angle = 360
    render.motion_blur_strength = "equal"
    render.motion_blur_gamma = False
    render.interpolateModel = "fake-rife"
    render.interpolateOption = Interpolator()
    render.sceneDetect = type("SceneDetector", (), {"detect": lambda self, frame: False})()
    render.duplicate_detector = FrameDuplicateDetector(threshold=0.9)
    render.extraRestorationModels = []
    render.upscaleModel = None
    render.override_upscale_scale = None
    render.width = 1
    render.height = 1
    render.readBuffer = Reader()
    render.writeBuffer = Writer()
    render.informationHandler = Information()

    render.render()

    output = render.writeBuffer.frames
    assert len(output) == 4  # Three input-rate frames plus queue terminator.
    assert [np.frombuffer(frame, dtype=np.uint8).tolist()[0] for frame in output[:-1]] == [0, 128, 255]
    assert output[-1] is None
    assert render.interpolateOption.transitions == [False, True, False]
    assert render.informationHandler.stopped
