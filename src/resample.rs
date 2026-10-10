// Copyright 2011, 2012 The Chromium Authors
// Chromium-compatible resampling adaptations: Copyright 2026 drawio-png-cli contributors.
// SPDX-License-Identifier: BSD-3-Clause
//! Half-size Hamming1 resampling matching Desktop capture on a 1x display.
//! Chromium's filter quantization and channel rounding are retained here;
//! see THIRD_PARTY_NOTICES.md for the reference revision and BSD license.
use anyhow::{Context, Result, ensure};
use std::{io::Cursor, time::Instant};

use crate::{MAX_BYTES, png_data};

pub(crate) const SCALE: u32 = 2;
const PRECISION: i32 = 1 << 14;

/// Bound the RGBA capture, including the twofold scale in both dimensions.
pub(crate) fn capture_size(width: u32, height: u32) -> Result<usize> {
    let pixels = u64::from(width) * u64::from(height);
    ensure!(
        width > 0 && height > 0 && pixels <= MAX_BYTES as u64 / 16,
        "2x Chromium capture exceeds 64 MiB limit"
    );
    Ok(pixels as usize * 16)
}

fn check_deadline(deadline: Instant) -> Result<()> {
    crate::interrupt::check()?;
    ensure!(
        Instant::now() < deadline,
        "Chromium rendering timed out during resizing"
    );
    Ok(())
}

pub(crate) fn half_png(
    bytes: &[u8],
    width: u32,
    height: u32,
    deadline: Instant,
) -> Result<Vec<u8>> {
    let size = capture_size(width, height)?;
    check_deadline(deadline)?;
    png_data::validate(bytes)?;
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits { bytes: MAX_BYTES });
    decoder.set_ignore_text_chunk(true);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info()?;
    ensure!(
        reader.info().width == width * SCALE && reader.info().height == height * SCALE,
        "Chromium screenshot dimensions do not match the 2x capture"
    );
    let mut decoded = vec![
        0;
        reader
            .output_buffer_size()
            .context("invalid capture size")?
    ];
    let info = reader.next_frame(&mut decoded)?;
    decoded.truncate(info.buffer_size());
    let mut pixels = Vec::with_capacity(size);
    // Work in premultiplied, encoded sRGB, as Chromium does. Transparent pixels
    // must not contribute their hidden RGB values to the result.
    for pixel in decoded.chunks_exact(info.color_type.samples()) {
        let (r, g, b, a) = match info.color_type {
            png::ColorType::Rgb => (pixel[0], pixel[1], pixel[2], 255),
            png::ColorType::Rgba => (pixel[0], pixel[1], pixel[2], pixel[3]),
            png::ColorType::Grayscale => (pixel[0], pixel[0], pixel[0], 255),
            png::ColorType::GrayscaleAlpha => (pixel[0], pixel[0], pixel[0], pixel[1]),
            png::ColorType::Indexed => unreachable!("palette was expanded"),
        };
        for c in [r, g, b] {
            pixels.push(((u32::from(c) * u32::from(a) + 127) / 255) as u8);
        }
        pixels.push(a);
    }
    drop(decoded);
    let pixels = half_rgba(&pixels, width as usize, height as usize, deadline)?;
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(&pixels)?;
    }
    ensure!(output.len() <= MAX_BYTES, "output PNG exceeds 64 MiB limit");
    check_deadline(deadline)?;
    Ok(output)
}

struct Kernel {
    start: usize,
    weights: Vec<i32>,
}

impl Kernel {
    fn new(length: usize, index: usize) -> Self {
        let center = 2 * index + 1;
        let start = center.saturating_sub(2);
        let end = (center + 2).min(length * 2 - 1);
        let samples: Vec<f32> = (start..=end)
            .map(|i| {
                let distance = (i as f32 + 0.5 - center as f32) * 0.5;
                if distance.abs() >= 1.0 {
                    0.0
                } else {
                    // A 2:1 pixel-center mapping never samples the origin.
                    let angle = distance * std::f32::consts::PI;
                    (angle.sin() / angle) * (0.54 + 0.46 * angle.cos())
                }
            })
            .collect();
        let sum: f32 = samples.iter().sum();
        let mut weights: Vec<i32> = samples
            .iter()
            .map(|v| (v / sum * PRECISION as f32) as i32)
            .collect();
        let remainder = PRECISION - weights.iter().sum::<i32>();
        let middle = weights.len() / 2;
        weights[middle] += remainder;
        Self { start, weights }
    }
}

/// At 2:1 the interior kernel repeats. Cache only the edge and interior kernels,
/// keeping filter storage bounded even for a very wide, short image.
struct Axis {
    length: usize,
    first: Kernel,
    last: Kernel,
    middle: Kernel,
}

impl Axis {
    fn new(length: usize) -> Self {
        Self {
            length,
            first: Kernel::new(length, 0),
            last: Kernel::new(length, length - 1),
            middle: Kernel::new(length, 1.min(length - 1)),
        }
    }
    fn at(&self, index: usize) -> (usize, &[i32]) {
        if index == 0 {
            let kernel = &self.first;
            (kernel.start, &kernel.weights)
        } else if index + 1 == self.length {
            let kernel = &self.last;
            (kernel.start, &kernel.weights)
        } else {
            (index * 2 - 1, &self.middle.weights)
        }
    }
}

fn convolve(pixels: &[u8], start: usize, stride: usize, weights: &[i32]) -> [u8; 4] {
    let mut sum = [0i32; 4];
    for (index, weight) in weights.iter().enumerate() {
        let pixel = &pixels[start + index * stride..][..4];
        for channel in 0..4 {
            sum[channel] += weight * i32::from(pixel[channel]);
        }
    }
    sum.map(|value| (value >> 14).clamp(0, 255) as u8)
}

fn half_rgba(pixels: &[u8], width: usize, height: usize, deadline: Instant) -> Result<Vec<u8>> {
    let horizontal = Axis::new(width);
    let vertical = Axis::new(height);
    let stride = width * 4;
    let mut rows = vec![0; stride * height * 2];
    for (y, row) in rows.chunks_exact_mut(stride).enumerate() {
        check_deadline(deadline)?;
        for (x, output) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            if x % 4096 == 0 {
                check_deadline(deadline)?;
            }
            let (start, weights) = horizontal.at(x);
            output.copy_from_slice(&convolve(pixels, y * stride * 2 + start * 4, 4, weights));
        }
    }
    let mut output = vec![0; stride * height];
    for (y, row) in output.chunks_exact_mut(stride).enumerate() {
        check_deadline(deadline)?;
        let (start, weights) = vertical.at(y);
        for (x, pixel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            if x % 4096 == 0 {
                check_deadline(deadline)?;
            }
            let mut color = convolve(&rows, start * stride + x * 4, stride, weights);
            // Keep a valid premultiplied pixel after channel quantization.
            let alpha = *color.iter().max().unwrap();
            color[3] = alpha;
            let inverse_alpha = if alpha == 0 {
                0.0
            } else {
                1.0 / (f32::from(alpha) * (1.0 / 255.0))
            };
            for value in &mut color[..3] {
                // Match the float-normalized unpremultiplication used for PNG output.
                *value = (f32::from(*value) * (1.0 / 255.0) * inverse_alpha * 255.0)
                    .round_ties_even() as u8;
            }
            pixel.copy_from_slice(&color);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn pixels(bytes: &[u8]) -> Vec<u8> {
        let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
        let mut data = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut data).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        data.truncate(info.buffer_size());
        data
    }

    #[test]
    fn matches_electron_hamming1_for_translucent_colors_and_edges() {
        let output = half_png(
            include_bytes!("../tests/fixtures/resize-input.png"),
            8,
            6,
            Instant::now() + Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(
            pixels(&output),
            pixels(include_bytes!("../tests/fixtures/resize-hamming1.png"))
        );
    }

    #[test]
    fn preserves_constant_colors_at_small_and_narrow_sizes() {
        for (width, height) in [(1, 1), (1, 9), (9, 1), (2, 2), (8, 6)] {
            let color = [17, 25, 33, 255];
            let input = color.repeat(width * height * 4);
            let output = half_rgba(
                &input,
                width,
                height,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap();
            assert_eq!(output, color.repeat(width * height));
        }
        assert_eq!(
            half_rgba(&[0; 16], 1, 1, Instant::now() + Duration::from_secs(5)).unwrap(),
            [0; 4]
        );
    }

    #[test]
    fn rejects_oversized_captures_dimension_mismatches_and_expired_deadlines() {
        assert_eq!(capture_size(2048, 2048).unwrap(), MAX_BYTES);
        for (width, height) in [(2049, 2048), (0, 1), (u32::MAX, u32::MAX)] {
            assert!(capture_size(width, height).is_err());
        }
        let png = include_bytes!("../tests/fixtures/resize-input.png");
        assert!(
            half_png(png, 9, 6, Instant::now() + Duration::from_secs(5))
                .unwrap_err()
                .to_string()
                .contains("dimensions")
        );
        assert!(
            half_png(png, 8, 6, Instant::now())
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
    }
}
