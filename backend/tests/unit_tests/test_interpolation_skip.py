import os
import sys
from contextlib import nullcontext

import torch

BACKEND_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
if BACKEND_DIR not in sys.path:
    sys.path.insert(0, BACKEND_DIR)

from src.pytorch.InterpolateGMFSS import InterpolateGMFSSTorch
from src.pytorch.InterpolateRIFE import InterpolateRifeTorch


class FakeFrame:
    def __init__(self, tensor=None):
        self.tensor = tensor if tensor is not None else torch.ones((1, 3, 8, 8))

    def get_frame_tensor(self):
        return self.tensor


class FakeTorchUtils:
    @staticmethod
    def run_stream(_stream):
        return nullcontext()

    @staticmethod
    def sync_stream(_stream):
        pass

    @staticmethod
    def sync_all_streams():
        pass

    @staticmethod
    def copy_tensor(destination, source, _stream):
        destination.copy_(source)


class ForbiddenFlow:
    def __call__(self, *_args, **_kwargs):
        raise AssertionError("optical-flow inference should be skipped")

    def forward(self, *_args, **_kwargs):
        raise AssertionError("optical-flow inference should be skipped")

    def reset_cache_after_transition(self):
        self.reset_count += 1

    def __init__(self):
        self.reset_count = 0


def _set_common_state(model, *, copy_stream_name):
    model.torchUtils = FakeTorchUtils()
    model.stream = object()
    model.prepareStream = object()
    setattr(model, copy_stream_name, object())
    model.frame0 = torch.zeros((1, 3, 8, 8))
    model.padding = (0, 0, 0, 0)
    model.ceilInterpolateFactor = 4
    model.height = 8
    model.width = 8
    model.flownet = ForbiddenFlow()
    model.dynamicScaledOpticalFlow = True
    model.dynamicScale = type(
        "ForbiddenDynamicScale",
        (),
        {"dynamicScaleCalculation": lambda *_args: (_ for _ in ()).throw(
            AssertionError("dynamic optical-flow calculation should be skipped")
        )},
    )()


def test_rife_transition_skips_flow_and_updates_frame_cache():
    model = InterpolateRifeTorch.__new__(InterpolateRifeTorch)
    _set_common_state(model, copy_stream_name="copyStream")
    model.encode = False

    frame = FakeFrame()
    assert list(model(frame, transition=True)) == [frame, frame, frame]
    assert torch.equal(model.frame0, frame.tensor)


def test_gmfss_transition_skips_flow_and_resets_cache_once():
    model = InterpolateGMFSSTorch.__new__(InterpolateGMFSSTorch)
    _set_common_state(model, copy_stream_name="copy_stream")

    frame = FakeFrame()
    assert list(model(frame, transition=True)) == [frame, frame, frame]
    assert torch.equal(model.frame0, frame.tensor)
    assert model.flownet.reset_count == 1
