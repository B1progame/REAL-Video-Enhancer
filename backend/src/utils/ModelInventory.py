"""Backend-only inventory of checkpoint architectures supported by this build."""

from __future__ import annotations


def get_supported_model_architectures() -> dict[str, list[str] | str]:
    """Return architecture names that the backend can auto-detect.

    This describes loader support, not availability of model weight files.
    Spandrel is vendored, while its optional runtime requirements (such as
    einops) are supplied by the backend dependency profile.
    """
    from ..pytorch.InterpolateArchs.DetectInterpolateArch import archs

    inventory: dict[str, list[str] | str] = {
        "interpolation": [architecture.__name__ for architecture in archs],
        "upscale_restoration": [],
        "temporal_upscaling": ["AnimeSR", "TSPAN"],
    }
    try:
        from ..pytorch.spandrel import MAIN_REGISTRY

        inventory["upscale_restoration"] = sorted(
            support.architecture.id for support in MAIN_REGISTRY.architectures()
        )
    except (ImportError, ModuleNotFoundError) as error:
        inventory["upscale_restoration"] = (
            f"Unavailable: backend architecture dependencies could not be imported ({error})"
        )

    return inventory
