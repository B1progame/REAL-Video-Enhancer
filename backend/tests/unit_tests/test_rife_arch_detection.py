import os
import sys
from collections import OrderedDict

import pytest
import torch

BACKEND_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
if BACKEND_DIR not in sys.path:
    sys.path.insert(0, BACKEND_DIR)

from src.pytorch.InterpolateArchs.DetectInterpolateArch import ArchDetect


@pytest.mark.parametrize(
    ("arch_name", "lastconv_key", "conv_input_channels"),
    [
        ("RIFE425", "module.block4.lastconv.0.bias", 15),
        ("RIFE425_heavy", "module.block0.lastconv.0.bias", 39),
    ],
)
def test_rife_425_checkpoint_shape_selects_correct_architecture(
    arch_name, lastconv_key, conv_input_channels
):
    # These shape signatures are the architecture discriminator used for real
    # RIFE 4.25 checkpoints; a tiny ordered state dict avoids loading weights.
    state_dict = OrderedDict(
        [
            (lastconv_key, torch.zeros(52)),
            (
                "module.block0.conv0.0.0.weight",
                torch.zeros((96, conv_input_channels, 3, 3)),
            ),
        ]
    )

    detected = ArchDetect(state_dict)

    assert detected.getArchName() == arch_name
    assert detected.getArchBase() == "rife"
