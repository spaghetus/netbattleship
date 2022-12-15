use std::{
	sync::mpsc::{Receiver, Sender},
	thread::JoinHandle,
};

fn main() {}

pub enum ThreadMsg {}

pub struct App {
	pub thread: JoinHandle<()>,
	pub send: Sender<ThreadMsg>,
	pub recv: Receiver<ThreadMsg>,
}

impl eframe::App for App {
	fn update(&mut self, _ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {}
}
