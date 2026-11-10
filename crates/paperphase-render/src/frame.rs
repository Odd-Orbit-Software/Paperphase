use std::io::Write;

use anyhow::{Result, ensure};
use paperphase_core::{DisplaySpec, Rect};

/// Row-major, MSB-first pixels: one means black ink, zero means white paper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    width: u32,
    height: u32,
    stride: usize,
    bits: Vec<u8>,
}

impl Frame {
    pub fn blank(display: &DisplaySpec) -> Result<Self> {
        display.validate()?;
        let stride = display.width.div_ceil(8) as usize;
        Ok(Self {
            width: display.width,
            height: display.height,
            stride,
            bits: vec![0; stride * display.height as usize],
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn bits(&self) -> &[u8] {
        &self.bits
    }

    pub fn is_black(&self, x: u32, y: u32) -> Option<bool> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(self.bits[y as usize * self.stride + x as usize / 8] & (0x80 >> (x % 8)) != 0)
    }

    pub(crate) fn ink(&mut self, x: i64, y: i64, clip: Rect) {
        if !clip.contains(x, y)
            || x < 0
            || y < 0
            || x >= i64::from(self.width)
            || y >= i64::from(self.height)
        {
            return;
        }
        self.bits[y as usize * self.stride + x as usize / 8] |= 0x80 >> (x % 8);
    }

    pub(crate) fn outline(&mut self, rect: Rect) {
        for x in rect.x..rect.right() as u32 {
            self.ink(i64::from(x), i64::from(rect.y), rect);
            self.ink(i64::from(x), rect.bottom() as i64 - 1, rect);
        }
        for y in rect.y..rect.bottom() as u32 {
            self.ink(i64::from(rect.x), i64::from(y), rect);
            self.ink(rect.right() as i64 - 1, i64::from(y), rect);
        }
    }

    /// One union rectangle of actual pixel changes. No panel policy is implied.
    pub fn difference_bounds(&self, previous: &Self) -> Result<Option<Rect>> {
        ensure!(
            self.width == previous.width && self.height == previous.height,
            "cannot compare different display dimensions"
        );
        let mut min_x = self.width;
        let mut min_y = self.height;
        let mut max_x = 0;
        let mut max_y = 0;
        for (i, (a, b)) in self.bits.iter().zip(&previous.bits).enumerate() {
            let different = a ^ b;
            if different == 0 {
                continue;
            }
            let y = (i / self.stride) as u32;
            let base_x = ((i % self.stride) * 8) as u32;
            for bit in 0..8 {
                let x = base_x + bit;
                if x < self.width && different & (0x80 >> bit) != 0 {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        Ok((min_x < self.width).then(|| Rect {
            x: min_x,
            y: min_y,
            width: max_x - min_x + 1,
            height: max_y - min_y + 1,
        }))
    }

    pub fn write_png(&self, output: impl Write) -> Result<()> {
        let mut encoder = png::Encoder::new(output, self.width, self.height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::One);
        let mut writer = encoder.write_header()?;
        // PNG uses zero for black; invert the same physical ink buffer.
        let pixels: Vec<_> = self.bits.iter().map(|b| !b).collect();
        writer.write_image_data(&pixels)?;
        writer.finish()?;
        Ok(())
    }

    pub fn write_pbm(&self, mut output: impl Write) -> Result<()> {
        write!(output, "P4\n{} {}\n", self.width, self.height)?;
        output.write_all(&self.bits)?;
        Ok(())
    }
}
