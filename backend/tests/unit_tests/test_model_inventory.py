import os
import sys

BACKEND_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
if BACKEND_DIR not in sys.path:
    sys.path.insert(0, BACKEND_DIR)

from src.utils.ModelInventory import get_supported_model_architectures


def test_model_inventory_reports_interpolation_and_temporal_architectures():
    inventory = get_supported_model_architectures()

    assert "RIFE425" in inventory["interpolation"]
    assert "RIFE425_heavy" in inventory["interpolation"]
    assert {"GMFSS", "GIMM", "IFRNET"}.issubset(inventory["interpolation"])
    assert inventory["temporal_upscaling"] == ["AnimeSR", "TSPAN"]
    assert isinstance(inventory["upscale_restoration"], (list, str))
