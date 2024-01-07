use std::error::Error as StdError;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use std::{env, time::Instant};

use ftdi_embedded_hal as hal;
use ftdi_embedded_hal::I2c;
use mlx9064x::{FrameRate, Mlx90640Driver};

pub struct Camera {
    camera: Mlx90640Driver<I2c<libftd2xx::Ft232h>>,
    last: Instant,
}

impl Camera {
    pub fn new(address: u8) -> Result<Self, ()> {
        let device = libftd2xx::Ft232h::with_serial_number("mlx90640").unwrap();
        let hal = hal::FtHal::init_freq(device, 1_000_000).unwrap();
        let i2c = hal.i2c().unwrap();
        //i2c.set_fast(true);

        let mut camera = Mlx90640Driver::new(i2c, address).unwrap();
        camera.set_frame_rate(FrameRate::Sixteen);
        let last = Instant::now();

        Ok(Camera { camera, last })
    }

    pub fn capture(&mut self) -> Result<Vec<f32>, ()> {
        let mut temperatures = vec![0f32; self.camera.height() * self.camera.width()];
        // refresh rate (32) - 20%
        let delay = Duration::from_millis(800 / 16);
        for i in 0..2 {
            loop {
                if self
                    .camera
                    .generate_image_if_ready(&mut temperatures)
                    .unwrap()
                {
                    break;
                }
            }
            if i == 0 {
                sleep(delay);
            }
        }
        let stamp = std::time::Instant::now();
        println!("latency: {:?}", stamp - self.last);
        self.last = stamp;
        Ok(temperatures)
    }
}
