import os
import sys

import numpy as np
import pytest


BACKEND_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
if BACKEND_DIR not in sys.path:
    sys.path.insert(0, BACKEND_DIR)

from src.utils.Frame import Frame
from src.utils.FrameDeduplicate import FrameDuplicateDetector


def make_frame(value: int) -> Frame:
    return Frame("onnx", 32, 24, "cpu", 0, False, "float32").set_frame_np(
        np.full((24, 32, 3), value, dtype=np.uint8)
    )


def test_detector_flags_duplicate_and_keeps_last_distinct_reference():
    detector = FrameDuplicateDetector(threshold=0.9)
    textured = np.indices((24, 32)).sum(axis=0) % 2 * 50 + 100
    first = Frame("onnx", 32, 24, "cpu", 0, False, "float32").set_frame_np(
        np.repeat(textured[..., None].astype(np.uint8), 3, axis=2)
    )
    slightly_changed = Frame("onnx", 32, 24, "cpu", 0, False, "float32").set_frame_np(
        np.repeat((textured + 1)[..., None].astype(np.uint8), 3, axis=2)
    )
    assert detector.is_duplicate(first) is False
    assert detector.is_duplicate(first.clone()) is True
    # Small changes to textured footage stay duplicates at the TAS-style
    # default threshold, while a scene-wide change is kept as a new reference.
    assert detector.is_duplicate(slightly_changed) is True
    assert detector.is_duplicate(make_frame(255)) is False


def test_detector_validates_threshold_and_sample_size():
    with pytest.raises(ValueError, match="between 0 and 1"):
        FrameDuplicateDetector(threshold=1.1)
    with pytest.raises(ValueError, match="at least 8"):
        FrameDuplicateDetector(sample_size=4)


def test_detector_does_not_merge_isoluminant_color_changes():
    detector = FrameDuplicateDetector(threshold=0.9)
    red = Frame("onnx", 32, 24, "cpu", 0, False, "float32").set_frame_np(
        np.tile(np.array([255, 0, 0], dtype=np.uint8), (24, 32, 1))
    )
    green = Frame("onnx", 32, 24, "cpu", 0, False, "float32").set_frame_np(
        np.tile(np.array([0, 130, 0], dtype=np.uint8), (24, 32, 1))
    )

    assert detector.is_duplicate(red) is False
    assert detector.is_duplicate(green) is False


def test_render_skips_inference_for_duplicate_interval_but_keeps_frame_count():
    from src.RenderVideo import Render

    class Reader:
        def __init__(self):
            self.frames = iter([make_frame(0), make_frame(0), make_frame(255), None])

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
            self.previous = img1.clone()
            if transition:
                return [img1]
            midpoint = np.full((24, 32, 3), 127, dtype=np.uint8)
            return [img1.get_dummy_frame().set_frame_np(midpoint)]

    render = Render.__new__(Render)
    render.motion_blur = False
    render.interpolateModel = "fake-rife"
    render.interpolateOption = Interpolator()
    render.sceneDetect = type("SceneDetector", (), {"detect": lambda self, frame: False})()
    render.duplicate_detector = FrameDuplicateDetector(threshold=0.99)
    render.extraRestorationModels = []
    render.upscaleModel = None
    render.override_upscale_scale = None
    render.width = 32
    render.height = 24
    render.readBuffer = Reader()
    render.writeBuffer = Writer()
    render.informationHandler = Information()

    render.render()

    # First input primes interpolation; the duplicate interval yields one
    # current-frame replacement plus its endpoint; the distinct interval gets
    # normal interpolation plus its endpoint. Queue includes a final sentinel.
    assert len(render.writeBuffer.frames) == 6
    assert render.interpolateOption.transitions == [False, True, False]
    assert render.writeBuffer.frames[-1] is None
    assert render.informationHandler.stopped
