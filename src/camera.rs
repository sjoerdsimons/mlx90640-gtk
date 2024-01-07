use anyhow::Result;
use std::error::Error as StdError;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use std::{env, time::Instant};

use ftdi_embedded_hal as hal;
use ftdi_embedded_hal::I2c;
use mlx9064x::{FrameRate, Mlx90640Driver};

pub trait Capture {
    fn capture(&mut self) -> anyhow::Result<Vec<f32>>;
    fn width(&self) -> u8;
    fn height(&self) -> u8;
}

pub struct Camera {
    camera: Mlx90640Driver<I2c<libftd2xx::Ft232h>>,
}

impl Camera {
    pub fn new(address: u8) -> anyhow::Result<Self> {
        let device = libftd2xx::Ft232h::with_serial_number("mlx90640")?;
        let hal = hal::FtHal::init_freq(device, 1_000_000)?;
        let i2c = hal.i2c().unwrap();
        //i2c.set_fast(true);

        let mut camera = Mlx90640Driver::new(i2c, address).unwrap();
        let _ = camera.set_frame_rate(FrameRate::Sixteen);
        Ok(Camera { camera })
    }
}

impl Capture for Camera {
    fn capture(&mut self) -> anyhow::Result<Vec<f32>> {
        let mut temperatures = vec![0f32; self.camera.height() * self.camera.width()];
        let delay = Duration::from_millis(1000 / 8 / 16);
        for _i in 0..2 {
            loop {
                if self.camera.generate_image_if_ready(&mut temperatures)? {
                    break;
                } else {
                    sleep(delay);
                }
            }
        }
        Ok(temperatures)
    }

    fn width(&self) -> u8 {
        32
    }

    fn height(&self) -> u8 {
        24
    }
}

pub struct MockCamera {}
impl MockCamera {
    pub fn new() -> Self {
        MockCamera {}
    }
}

impl Capture for MockCamera {
    fn capture(&mut self) -> anyhow::Result<Vec<f32>> {
        sleep(Duration::from_millis(200));
        let mut v = Vec::with_capacity(self.width() as usize * self.height() as usize);
        for x in 0..self.width() {
            for y in 0..self.height() {
                v.push(19.0 + x as f32 / 4.0 + y as f32 / 8.0);
            }
        }

        Ok(v)
    }

    fn width(&self) -> u8 {
        32
    }

    fn height(&self) -> u8 {
        24
    }
}
