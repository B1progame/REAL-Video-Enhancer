"""Small, backend-neutral helpers for interpolation-based motion blur."""

from __future__ import annotations

import math
from typing import Sequence

import numpy as np


WEIGHT_SCHEMES = ("gaussian_sym", "equal", "pyramid", "ascending", "descending")


def generate_weights(count: int, scheme: str = "gaussian_sym") -> list[float]:
    """Create normalized temporal weights for a sequence of frame samples."""
    if count <= 0:
        return []
    if scheme not in WEIGHT_SCHEMES:
        raise ValueError(f"Unknown motion-blur weight scheme: {scheme}")
    if count == 1:
        return [1.0]

    center = (count - 1) / 2.0
    if scheme == "gaussian_sym":
        sigma = max(count / 4.0, 0.5)
        values = [math.exp(-((i - center) ** 2) / (2.0 * sigma**2)) for i in range(count)]
    elif scheme == "pyramid":
        values = [max(0.0, 1.0 - abs(i - center) / (center + 1.0)) for i in range(count)]
    elif scheme == "ascending":
        values = [float(i + 1) for i in range(count)]
    elif scheme == "descending":
        values = [float(count - i) for i in range(count)]
    else:
        values = [1.0] * count

    total = sum(values)
    return [value / total for value in values]


def select_shutter_samples(frames: Sequence, shutter_angle: float) -> list:
    """Select the shutter window ending at the output frame's timestamp."""
    if not frames:
        raise ValueError("At least one frame sample is required")
    angle = float(shutter_angle)
    if not 0.0 <= angle <= 360.0:
        raise ValueError("Shutter angle must be between 0 and 360 degrees")
    if len(frames) == 1 or angle == 360.0:
        return list(frames)

    lower = 1.0 - angle / 360.0
    upper = 1.0
    denominator = len(frames) - 1
    selected = [
        frame
        for i, frame in enumerate(frames)
        if lower - 1e-9 <= i / denominator <= upper + 1e-9
    ]
    if selected:
        return selected
    return [frames[-1]]


def blend_arrays(
    frames: Sequence[np.ndarray],
    weights: Sequence[float] | None = None,
    *,
    gamma_correct: bool = True,
) -> np.ndarray:
    """Blend same-shaped RGB arrays, optionally averaging SDR in linear light."""
    if not frames:
        raise ValueError("At least one frame is required")
    first = np.asarray(frames[0])
    if first.dtype not in (np.dtype(np.uint8), np.dtype(np.uint16)):
        raise TypeError("Motion-blur frames must use uint8 or uint16 samples")
    if any(np.asarray(frame).shape != first.shape for frame in frames):
        raise ValueError("All motion-blur frames must have the same shape")

    if weights is None:
        weights = generate_weights(len(frames))
    if len(weights) != len(frames):
        raise ValueError("The number of weights must match the number of frames")
    total = float(sum(weights))
    if not math.isfinite(total) or total <= 0 or any(w < 0 for w in weights):
        raise ValueError("Weights must be finite, nonnegative, and have a positive sum")
    normalized = [float(weight) / total for weight in weights]

    max_value = float(np.iinfo(first.dtype).max)
    stack = np.stack([np.asarray(frame, dtype=np.float32) / max_value for frame in frames])
    if gamma_correct and first.dtype == np.uint8:
        stack = np.power(np.clip(stack, 0.0, 1.0), 2.2)

    weight_array = np.asarray(normalized, dtype=np.float32).reshape(
        (len(normalized),) + (1,) * (stack.ndim - 1)
    )
    blended = np.sum(stack * weight_array, axis=0)
    if gamma_correct and first.dtype == np.uint8:
        blended = np.power(np.clip(blended, 0.0, 1.0), 1.0 / 2.2)
    return np.rint(np.clip(blended, 0.0, 1.0) * max_value).astype(first.dtype)


def blend_frame_samples(
    frames: Sequence,
    *,
    shutter_angle: float = 180.0,
    strength: str = "gaussian_sym",
    gamma_correct: bool = True,
):
    """Blend selected Frame objects and return a new Frame on the same backend."""
    selected = select_shutter_samples(frames, shutter_angle)
    arrays = [frame.get_frame_np() for frame in selected]
    result = blend_arrays(arrays, generate_weights(len(arrays), strength), gamma_correct=gamma_correct)
    output = frames[-1].get_dummy_frame()
    output.set_frame_np(result)
    return output
