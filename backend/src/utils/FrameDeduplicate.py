"""Lightweight consecutive-frame duplicate detection for the render pipeline."""

from __future__ import annotations

import cv2
import numpy as np


class FrameDuplicateDetector:
    """Detect near-duplicate frames using SSIM on a small RGB preview.

    The reference is advanced only when a distinct frame is seen. This avoids
    accumulating small changes across a slow pan or fade and accidentally
    classifying the entire motion as duplicate content.
    """

    def __init__(self, threshold: float = 0.9, sample_size: int = 224):
        if not 0.0 <= float(threshold) <= 1.0:
            raise ValueError("Duplicate SSIM threshold must be between 0 and 1")
        if int(sample_size) < 8:
            raise ValueError("Duplicate detector sample size must be at least 8")
        self.threshold = float(threshold)
        self.sample_size = int(sample_size)
        self._previous = None

    def _prepare(self, frame) -> np.ndarray:
        width = min(self.sample_size, max(1, frame.width))
        height = min(self.sample_size, max(1, frame.height))
        image = frame.get_np_sdr_resized(width, height)
        if image.ndim == 3 and image.shape[2] == 4:
            image = image[:, :, :3]
        elif image.ndim == 3 and image.shape[2] == 1:
            image = image[:, :, 0]
        return image.astype(np.float32, copy=False)

    @staticmethod
    def _ssim(image_a: np.ndarray, image_b: np.ndarray) -> float:
        # Standard local SSIM, with 8-bit image constants. The low-resolution
        # sample keeps this inexpensive even when the source is 4K or larger.
        c1 = (0.01 * 255.0) ** 2
        c2 = (0.03 * 255.0) ** 2
        mu_a = cv2.GaussianBlur(image_a, (11, 11), 1.5)
        mu_b = cv2.GaussianBlur(image_b, (11, 11), 1.5)
        mu_a_sq = mu_a * mu_a
        mu_b_sq = mu_b * mu_b
        mu_ab = mu_a * mu_b
        sigma_a_sq = cv2.GaussianBlur(image_a * image_a, (11, 11), 1.5) - mu_a_sq
        sigma_b_sq = cv2.GaussianBlur(image_b * image_b, (11, 11), 1.5) - mu_b_sq
        sigma_ab = cv2.GaussianBlur(image_a * image_b, (11, 11), 1.5) - mu_ab
        numerator = (2.0 * mu_ab + c1) * (2.0 * sigma_ab + c2)
        denominator = (mu_a_sq + mu_b_sq + c1) * (sigma_a_sq + sigma_b_sq + c2)
        return float(np.mean(numerator / np.maximum(denominator, 1e-12)))

    def is_duplicate(self, frame) -> bool:
        sample = self._prepare(frame)
        if self._previous is None:
            self._previous = sample
            return False
        score = self._ssim(self._previous, sample)
        if score < self.threshold:
            self._previous = sample
            return False
        return True
