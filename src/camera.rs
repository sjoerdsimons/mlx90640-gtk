use std::env;
use std::error::Error as StdError;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use ftdi_embedded_hal as hal;
use ftdi_embedded_hal::I2c;
use mlx9064x::{FrameRate, Mlx90640Driver};

pub struct Camera {
    camera: Mlx90640Driver<I2c<ftdi::Device>>,
}

impl Camera {
    pub fn new(address: u8) -> Result<Self, ()> {
        let mut device = ftdi::find_by_vid_pid(0x0403, 0x6014)
            .interface(ftdi::Interface::A)
            .open()
            .unwrap();
        device.set_latency_timer(1);
        let hal = hal::FtHal::init_freq(device, 400_000).unwrap();
        let mut i2c = hal.i2c().unwrap();
        i2c.set_fast(true);

        let camera = Mlx90640Driver::new(i2c, address).unwrap();

        Ok(Camera { camera })
    }

    pub fn capture(&mut self) -> Result<Vec<f32>, ()> {
        self.camera.set_frame_rate(FrameRate::ThirtyTwo);
        let mut temperatures = vec![0f32; self.camera.height() * self.camera.width()];
        //let delay = Duration::from_millis(100);
        for _ in 0..2 {
            loop {
                if self
                    .camera
                    .generate_image_if_ready(&mut temperatures)
                    .unwrap()
                {
                    break;
                }
            }
        }
        Ok(temperatures)
    }
}
