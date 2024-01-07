use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use futures::StreamExt;
use gtk::cairo::Context as CairoContext;
use gtk::gio;
use gtk::glib::clone;
use gtk::{glib, Application, ApplicationWindow, Button};
use gtk::{prelude::*, DrawingArea};
use gtk4 as gtk;

mod camera;

#[derive(Clone, Debug)]
struct LastCapture {
    capture: Rc<RefCell<Option<Vec<f32>>>>,
}

impl LastCapture {
    fn new() -> LastCapture {
        LastCapture {
            capture: Rc::new(RefCell::default()),
        }
    }

    fn update(&self, capture: Vec<f32>) {
        *self.capture.borrow_mut() = Some(capture);
    }

    fn get(&self) -> std::cell::Ref<'_, Option<Vec<f32>>> {
        self.capture.borrow()
    }
}

fn draw(
    capture: &LastCapture,
    _area: &DrawingArea,
    context: &CairoContext,
    width: i32,
    height: i32,
) {
    println!("draw draw: {width}x{height}");

    //context.set_source_rgb(0.0, 0.0, 0.0);
    //context.paint().unwrap();
    context.scale(width as f64 / 32.0, height as f64 / 24.0);
    if let Some(capture) = capture.get().as_ref() {
        for y in 0..24 {
            //println!("");
            for x in 0..32 {
                //println!("{:?}", &capture[x * 24..(x + 1) * 24]);
                let temp = capture[x + (y * 32)];
                let r = ((temp - 20.0) / 10.0) as f64;
                //print!("{:2.1} ", temp);
                context.rectangle(x as f64, y as f64, 1.0, 1.0);
                context.set_source_rgb(r, 0.0, 0.0);
                context.fill().unwrap();
            }
        }
    } else {
        for x in 0..32 {
            for y in 0..24 {
                let (r, g, b) = if (x + y) % 2 == 0 {
                    (1.0, 1.0, 1.0)
                } else {
                    (0.0, 0.0, 0.0)
                };

                context.rectangle(x as f64, y as f64, 1.0, 1.0);
                context.set_source_rgb(r, g, b);
                context.fill().unwrap();
            }
        }
    }
}

fn main() -> glib::ExitCode {
    let application = Application::builder()
        .application_id("one.simons.MLX90640")
        .build();

    application.connect_activate(|app| {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("MLX90640")
            .build();

        let drawing_area = gtk::DrawingArea::builder()
            .content_width(320)
            .content_height(240)
            .build();

        let last = LastCapture::new();
        let last_c = last.clone();

        drawing_area.set_draw_func(move |d, c, w, h| draw(&last_c, d, c, w, h));

        let (mut sender, mut receiver) = futures::channel::mpsc::channel(1);
        glib::spawn_future_local(clone!(@weak drawing_area, @strong last => async move {
            while let Some(capture) = receiver.next().await {
                last.update(capture);
                drawing_area.queue_draw();
            }
        }));

        gio::spawn_blocking(move || {
            let mut camera = camera::Camera::new(0x33).unwrap();
            loop {
                let capture = camera.capture().unwrap();
                sender.try_send(capture);
            }
        });

        let hbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .build();

        let button = Button::with_label("Click me!");
        button.connect_clicked(|_| {
            eprintln!("Clicked!");
        });

        hbox.append(&drawing_area);
        hbox.append(&button);

        window.set_child(Some(&hbox));

        window.present();
    });

    application.run()
}

