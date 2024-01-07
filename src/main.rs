use std::cell::{Ref, RefCell};
use std::path::Path;
use std::rc::Rc;

use clap::Parser;
use event_listener::Event;
use futures::StreamExt;
use gtk::cairo::Context as CairoContext;
use gtk::glib::clone;
use gtk::{gdk, gio, EventControllerMotion, Grid, Label};
use gtk::{glib, Application, ApplicationWindow, Button};
use gtk::{prelude::*, DrawingArea};
use gtk4 as gtk;

mod camera;
use camera::Capture;

#[derive(Clone, Debug)]
enum CameraType {
    Mock,
    MLX90640,
}

struct CameraEvent {
    event: Rc<Event>,
}

impl CameraEvent {
    async fn wait(&self) {
        self.event.listen().await
    }
}

#[derive(Debug)]
struct CameraCapture {
    data: Vec<f32>,
    lower: f32,
    upper: f32,
}

impl CameraCapture {
    fn new(data: Vec<f32>) -> Self {
        let min = data
            .iter()
            .fold(f32::NAN, |acc, item| if acc < *item { acc } else { *item });

        let lower = (min as i32 / 5) as f32 * 5.0;

        let max = data
            .iter()
            .fold(f32::NAN, |acc, item| if acc > *item { acc } else { *item });

        let upper = (max as i32 / 5 + 1) as f32 * 5.0;
        CameraCapture { data, lower, upper }
    }
}

impl std::ops::Index<usize> for CameraCapture {
    type Output = f32;

    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}

#[derive(Clone, Debug)]
struct CameraWrapper {
    camera_type: CameraType,
    last: Rc<RefCell<Option<CameraCapture>>>,
    event: Rc<Event>,
}

impl CameraWrapper {
    fn new() -> Self {
        CameraWrapper {
            camera_type: CameraType::MLX90640,
            last: Rc::new(RefCell::default()),
            event: Rc::new(Event::new()),
        }
    }

    fn new_mock() -> Self {
        CameraWrapper {
            camera_type: CameraType::Mock,
            last: Rc::new(RefCell::default()),
            event: Rc::new(Event::new()),
        }
    }

    fn start(&self) -> anyhow::Result<()> {
        let mut camera: Box<dyn camera::Capture + Send> = match self.camera_type {
            CameraType::Mock => Box::new(camera::MockCamera::new()),
            CameraType::MLX90640 => Box::new(camera::Camera::new(0x33)?),
        };

        let (mut sender, mut receiver) = futures::channel::mpsc::channel(2);
        gio::spawn_blocking(move || loop {
            match camera.capture() {
                Ok(mut capture) => loop {
                    match sender.try_send(capture) {
                        Ok(_) => break,
                        Err(e) => capture = e.into_inner(),
                    }
                },
                Err(e) => eprintln!("capture failed: {e}"),
            }
        });

        let wrapper = self.clone();
        glib::spawn_future_local(async move {
            while let Some(data) = receiver.next().await {
                let capture = CameraCapture::new(data);
                *wrapper.last.borrow_mut() = Some(capture);
                wrapper.event.notify(usize::MAX);
            }
        });
        Ok(())
    }

    fn event(&self) -> CameraEvent {
        let event = self.event.clone();
        CameraEvent { event }
    }

    fn get_last(&self) -> Ref<'_, Option<CameraCapture>> {
        self.last.borrow()
    }
}

fn temp_to_color(capture: &CameraCapture, temp: f32) -> (f64, f64, f64) {
    let min = capture.lower;
    let range = capture.upper - capture.lower;

    // HSV 0.0 is middle red; 240 degrees aka 0.6666 is blue
    let i = (temp.clamp(min, min + range) - min) / range;
    let h = if i > 0.66666666666 {
        1.0 - i + 0.66666666
    } else {
        0.666666666 - i
    };
    let (r, g, b) = gtk4::hsv_to_rgb(h, 0.9, 0.9);
    (r.into(), g.into(), b.into())
}

#[derive(Clone)]
struct OutputImage {
    drawing_area: DrawingArea,
    pos: Rc<RefCell<Option<(f64, f64)>>>,
    camera: CameraWrapper,
}

impl OutputImage {
    fn new(camera: CameraWrapper) -> Self {
        let drawing_area = gtk::DrawingArea::builder()
            .content_width(320)
            .content_height(240)
            .vexpand(true)
            .hexpand(true)
            .build();

        let me = OutputImage {
            drawing_area: drawing_area.clone(),
            pos: Rc::new(RefCell::new(None)),
            camera,
        };

        let event = EventControllerMotion::builder().name("drawing").build();
        event.connect_motion(clone!(@strong me => move |_, x, y| me.motion(x,y)));
        drawing_area.add_controller(event);

        drawing_area.set_draw_func(clone!(@strong me => move |_d, c, w, h| me.draw(c, w, h)));

        glib::spawn_future_local(clone!(@strong me => async move {
            let event = me.camera.event();
            loop { event.wait().await;
                me.update()
            }
        }));
        me
    }

    fn area(&self) -> &DrawingArea {
        &self.drawing_area
    }

    fn update(&self) {
        self.drawing_area.queue_draw();
    }

    fn index_at_cursor(&self) -> Option<usize> {
        let (x, y) = (*self.pos.borrow())?;
        let width = self.drawing_area.width();
        let height = self.drawing_area.height();
        let x = x.round() as i32;
        let y = y.round() as i32;
        let x = (x * 32) / width;
        let y = (y * 24) / height;
        Some((x + y * 32) as usize)
    }

    fn temp_at_cursor(&self) -> Option<f32> {
        let last = self.camera.get_last();
        let capture = last.as_ref()?;
        let index = self.index_at_cursor()?;
        capture.data.get(index).copied()
    }

    fn motion(&self, x: f64, y: f64) {
        *self.pos.borrow_mut() = Some((x, y));
        if let Some(t) = self.temp_at_cursor() {
            println!("T: {t}");
        }
    }

    fn draw(&self, context: &CairoContext, width: i32, height: i32) {
        context.scale(width as f64 / 32.0, height as f64 / 24.0);
        // NOTE drawing x-mirrored
        if let Some(capture) = self.camera.get_last().as_ref() {
            for y in 0..24 {
                for x in 0..32 {
                    let temp = capture[(31 - x) + (y * 32)];
                    let (r, g, b) = temp_to_color(capture, temp);
                    context.rectangle(x as f64, y as f64, 1.0, 1.0);
                    context.set_source_rgb(r, g, b);
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

        // draw cursor
        if let Some(pos) = self.index_at_cursor() {
            let x = pos % 32;
            let y = pos / 32;

            context.rectangle(x as f64, y as f64, 0.5, 0.5);
            context.set_source_rgb(1.0, 1.0, 1.0);
            context.fill().unwrap();
        }

        if let Some(t) = self.temp_at_cursor() {
            println!("T cursor: {t}");
        }
    }
}

fn draw_legend(
    wrapper: &CameraWrapper,
    _area: &DrawingArea,
    context: &CairoContext,
    width: i32,
    height: i32,
) {
    let last = wrapper.get_last();
    let Some(capture) = last.as_ref() else { return };
    let range = capture.upper - capture.lower;
    for y in 0..height {
        let i = y as f32 / height as f32;
        let temp = capture.upper - (range * i);
        let (r, g, b) = temp_to_color(capture, temp);

        context.set_source_rgb(r, g, b);
        context.move_to(0.0, y as f64);
        context.line_to((width) as f64, y as f64);
        context.set_line_width(1.0);
        context.stroke().unwrap();
    }
}

#[derive(clap::Parser, Debug)]
struct Opts {
    /// Use mock camera
    #[clap(short, long)]
    mock: bool,
}

fn main() -> glib::ExitCode {
    let opts = Opts::parse();
    dbg!(&opts);

    let wrapper = if opts.mock {
        CameraWrapper::new_mock()
    } else {
        CameraWrapper::new()
    };

    let application = Application::builder()
        .application_id("one.simons.MLX90640")
        .build();

    application.connect_activate(move |app| {
        let window = ApplicationWindow::builder()
            .application(app)
            .title("MLX90640")
            .build();

        let hbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .hexpand(true)
            .build();
        let vbox = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .hexpand(true)
            .build();
        hbox.append(&vbox);

        let output = OutputImage::new(wrapper.clone());
        vbox.append(output.area());

        // Legend
        let legend = gtk::DrawingArea::builder()
            .content_height(240)
            .content_width(20)
            .vexpand(true)
            .build();

        let wrapper_d = wrapper.clone();

        legend.set_draw_func(move |d, c, w, h| draw_legend(&wrapper_d, d, c, w, h));
        let event = wrapper.event();
        glib::spawn_future_local(clone!(@weak legend => async move {
            loop { event.wait().await;
                legend.queue_draw()
            }
        }));
        vbox.append(&legend);

        let grid = Grid::builder().row_homogeneous(true).build();

        let upper_label = Label::new(Some("upper"));
        let event = wrapper.event();
        glib::spawn_future_local(clone!(@strong wrapper, @weak upper_label => async move {
            loop {
                event.wait().await;
                if let Some(capture) = wrapper.get_last().as_ref() {
                    upper_label.set_text(&format!("{}", capture.upper));
                }
            }
        }));

        let lower_label = Label::new(Some("lower"));
        let event = wrapper.event();
        glib::spawn_future_local(clone!(@strong wrapper, @weak lower_label => async move {
            loop {
                event.wait().await;
                if let Some(capture) = wrapper.get_last().as_ref() {
                    lower_label.set_text(&format!("{}", capture.lower));
                }
            }
        }));

        grid.attach(&upper_label, 0, 0, 1, 1);
        grid.attach(&lower_label, 0, 1, 1, 1);
        vbox.append(&grid);

        let button = Button::with_label("Click me!");
        button.connect_clicked(|_| {
            eprintln!("Clicked!");
        });

        hbox.append(&button);

        window.set_child(Some(&hbox));

        wrapper.start().unwrap();
        window.present();
    });

    application.run_with_args::<&str>(&[])
}
