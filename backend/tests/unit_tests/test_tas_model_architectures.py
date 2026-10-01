import os
import sys

import pytest
import torch

BACKEND_PYTORCH_DIR = os.path.abspath(
    os.path.join(os.path.dirname(__file__), "..", "..", "src", "pytorch")
)
if BACKEND_PYTORCH_DIR not in sys.path:
    sys.path.insert(0, BACKEND_PYTORCH_DIR)


@pytest.mark.parametrize(
    ("architecture", "model_kwargs", "input_shape", "expected_scale"),
    [
        ("DIS", {"num_features": 8, "num_blocks": 1, "scale": 2}, (1, 3, 8, 8), 2),
        (
            "FIGSR",
            {
                "dim": 48,
                "scale": 2,
                "mid_dim": 16,
                "n_blocks": 2,
                "gc": 8,
                "square_kernel_size": 3,
                "band_kernel_size": 3,
            },
            (1, 3, 8, 8),
            2,
        ),
        (
            "GateRV3",
            {
                "dim": 8,
                "enc_blocks": (1, 1, 1, 1),
                "dec_blocks": (1, 1, 1, 1),
                "num_latent": 2,
                "scale": 1,
            },
            (1, 3, 16, 16),
            1,
        ),
        (
            "SMoSR",
            {"dim": 8, "scale": 2, "n_mb": 1, "upsampler_mid_dim": 8},
            (1, 3, 8, 8),
            2,
        ),
    ],
)
def test_tas_architecture_loads_checkpoint_and_runs(
    architecture, model_kwargs, input_shape, expected_scale
):
    # Importing the full Spandrel registry imports CRAFT, which depends on the
    # backend's declared einops requirement even though these four arches do not.
    pytest.importorskip("einops")
    from spandrel import MAIN_REGISTRY
    from spandrel.architectures.DIS.__arch.DIS import DIS
    from spandrel.architectures.FIGSR.__arch.figsr import FIGSR
    from spandrel.architectures.GateRV3.__arch.gaterv3 import GateRV3
    from spandrel.architectures.SMoSR.__arch.smosr import SMoSR

    support = MAIN_REGISTRY.get(architecture)
    assert support is not None
    assert support.architecture.id == architecture

    # Build a small valid model and use its weights as a synthetic checkpoint.
    # FIGSR needs at least two blocks because its body is split into two groups.
    model_class = {
        "DIS": DIS,
        "FIGSR": FIGSR,
        "GateRV3": GateRV3,
        "SMoSR": SMoSR,
    }[architecture]
    checkpoint = model_class(**model_kwargs).state_dict()

    descriptor = MAIN_REGISTRY.load(checkpoint)
    assert descriptor.architecture.id == architecture
    assert descriptor.scale == expected_scale

    output = descriptor(torch.rand(input_shape))
    assert output.shape == (
        input_shape[0],
        descriptor.output_channels,
        input_shape[2] * expected_scale,
        input_shape[3] * expected_scale,
    )
