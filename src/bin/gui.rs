use std::{net::SocketAddrV4, sync::Arc, time::Duration};

use eframe::{
	egui::{self, Button, Ui},
	epaint::{Color32, Vec2},
	NativeOptions,
};
use netbattleship::{
	flow::{GameFlow, TurnResults},
	ship::Ship,
	Phase,
};
use tokio::{runtime::Runtime, spawn, sync::RwLock, task::JoinHandle};

fn main() {
	eframe::run_native(
		"netbattleship",
		NativeOptions::default(),
		Box::new(|_| Box::new(App::default())),
	);
}

pub struct App {
	game: Arc<RwLock<Option<GameFlow>>>,
	msg: Arc<RwLock<Vec<String>>>,
	addr: String,
	serve: bool,
	task: Option<JoinHandle<()>>,
	runtime: Arc<Runtime>,
	last_result: Arc<RwLock<Option<TurnResults>>>,
	vertical: bool,
}

impl Default for App {
	fn default() -> Self {
		Self {
			game: Default::default(),
			msg: Default::default(),
			addr: Default::default(),
			serve: Default::default(),
			task: Default::default(),
			last_result: Default::default(),
			vertical: false,
			runtime: Arc::new(Runtime::new().expect("Failed to open runtime!")),
		}
	}
}

impl eframe::App for App {
	fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
		ctx.request_repaint_after(Duration::from_secs(1));

		egui::TopBottomPanel::bottom("messages").show(ctx, |ui| {
			for msg in self
				.runtime
				.block_on(self.msg.read())
				.iter()
				.rev()
				.take(10)
				.rev()
			{
				ui.label(msg);
				ui.separator();
			}
		});

		egui::TopBottomPanel::top("settings").show(ctx, |ui| {
			egui::widgets::global_dark_light_mode_switch(ui);
		});

		egui::CentralPanel::default().show(ctx, |ui| {
			// Clone the runtime for borrow checker reasons
			let runtime = self.runtime.clone();

			// Draw board
			let clicked = runtime.block_on(self.board(ui));
			ui.separator();

			// Learn the current phase of the game
			let phase = runtime
				.block_on(self.game.read())
				.as_ref()
				.map(|v| runtime.block_on(v.phase()));

			// Run different phase routines.
			match phase {
				None => runtime.block_on(self.setup(ui)),
				Some(phase) => match phase {
					Phase::Connecting => {
						ui.label("Connecting...");
					}
					Phase::Placing(ship) => runtime.block_on(self.placing(ui, clicked, ship)),
					Phase::Playing => runtime.block_on(self.playing(ui, clicked)),
					Phase::Done(won) => {
						if won {
							ui.heading("You won!");
						} else {
							ui.heading("You lost...");
						}
						if ui.button("Quit the game").clicked() {
							panic!("User requested exit, not a bug.")
						}
					}
				},
			}
		});
		// Update Futures
		if self.task.is_some() && self.task.as_ref().map(|t| t.is_finished()).unwrap_or(false) {
			self.runtime
				.block_on(self.task.take().unwrap())
				.expect("Background Task Panicked");
		}
	}
}

impl App {
	pub async fn setup(&mut self, ui: &mut Ui) {
		if self.game.read().await.is_none() && self.task.is_none() {
			ui.label("Socket Address");
			ui.text_edit_singleline(&mut self.addr);
			ui.checkbox(&mut self.serve, "Hosting?");
			if let Ok(addr) = self.addr.parse::<SocketAddrV4>() {
				if ui.button("Go!").clicked() {
					let addr = addr;
					let serve = self.serve;
					let game = self.game.clone();
					let msg = self.msg.clone();
					self.task = Some(spawn(async move {
						if serve {
							msg.write()
								.await
								.push(format!("Waiting for a challenger on {}...", addr))
						} else {
							msg.write().await.push(format!("Connecting to {}...", addr))
						}
						let new_game = GameFlow::new(std::net::SocketAddr::V4(addr), serve).await;

						match new_game {
							Ok(new_game) => {
								msg.write().await.push("Connected!".to_string());
								*game.write().await = Some(new_game)
							}
							Err(e) => msg.write().await.push(format!("{}", e)),
						}
					}))
				}
			} else {
				ui.colored_label(Color32::from_rgb(255, 0, 0), "Invalid address");
			}
		}
	}

	pub async fn board(&self, ui: &mut Ui) -> Option<(bool, u8, u8)> {
		if let Some(game) = &*self.game.read().await {
			let mut boards = Vec::with_capacity(2);
			for team in [false, true] {
				boards.push((team, game.board(team).await));
			}
			ui.horizontal_centered(|ui| {
				let mut out = None;
				for (team, board) in boards {
					if team {
						ui.separator();
					}
					for col in 0..10 {
						let response = ui
							.vertical(|ui| {
								for cell in 0..10 {
									if ui
										.add(
											Button::new(
												char::from(
													*board
														.board
														.get(&(col, cell))
														.unwrap_or(&Ship::None),
												)
												.to_string(),
											)
											.min_size(Vec2::new(16.0, 0.0)),
										)
										.clicked()
									{
										out = Some((team, col, cell));
									}
								}
								None
							})
							.inner;
						if response.is_some() {
							return response;
						}
					}
				}
				out
			})
			.inner
		} else {
			None
		}
	}

	pub async fn placing(&mut self, ui: &mut Ui, clicked: Option<(bool, u8, u8)>, ship: Ship) {
		// Name of ship
		ui.heading(format!("Placing {:?}.", ship));
		// Vertical Checkbox
		ui.checkbox(
			&mut self.vertical,
			"Vertical? (Check before placing.)".to_owned(),
		);

		if let Some(clicked) = clicked {
			if clicked.0 {
				self.msg
					.write()
					.await
					.push("You can't place your ship on the enemy's territory.".to_owned());
				return;
			}
			let game = self.game.write().await;
			let game = game.as_ref().unwrap();
			if game
				.place_ship(ship, (clicked.1, clicked.2), self.vertical)
				.await
				.is_ok()
			{
				self.msg.write().await.push("OK!".to_owned());
			} else {
				self.msg
					.write()
					.await
					.push("Bad placement... Try again!".to_owned());
			}
		}
	}

	pub async fn playing(&mut self, ui: &mut Ui, clicked: Option<(bool, u8, u8)>) {
		let our_turn = self.game.read().await.as_ref().unwrap().my_turn().await;
		if our_turn {
			ui.heading("Your Turn!");
			if self.task.is_none() {
				ui.label("Click on the enemy's board to fire.");
			} else {
				ui.label("Waiting on the enemy's response...");
				return;
			}
			if let Some(clicked) = clicked {
				if !clicked.0 {
					self.msg
						.write()
						.await
						.push("You can't fire on your own board.".to_owned());
					return;
				}

				let pos = (clicked.1, clicked.2);
				let game = self.game.clone();
				let last_result = self.last_result.clone();
				let msg = self.msg.clone();
				self.task = Some(spawn(async move {
					let game = game.read().await;
					let results = game.as_ref().unwrap().fire(pos).await;

					match results {
						Ok(tr) => {
							let mut msgs = vec![];
							msgs.push(format!(
								"You {} the enemy's ship at {}.",
								if tr.hit.is_some() { "hit" } else { "missed" },
								format_args!("({}, {})", tr.aim.0, tr.aim.1)
							));
							if tr.hit.is_some() {
								msgs.push(format!(
									"You {} the enemy's {}.",
									if tr.sunk.is_some() {
										"sunk"
									} else {
										"failed to sink"
									},
									tr.sunk
										.filter(|s| !s.is_empty())
										.map(|s| format!("{:?}", s))
										.unwrap_or_else(|| "ship".to_string()),
								))
							}
							msg.write().await.append(&mut msgs);
							*last_result.write().await = Some(tr)
						}
						Err(e) => msg.write().await.push(format!("{}", e)),
					};
				}))
			}
		} else {
			ui.heading("The enemy's turn.");
			if self.task.is_none() {
				let game = self.game.clone();
				let last_result = self.last_result.clone();
				let msg = self.msg.clone();
				self.task = Some(spawn(async move {
					let game = game.read().await;
					let result = game.as_ref().unwrap().receive().await.unwrap();
					let mut msgs = vec![];
					msgs.push(format!(
						"The enemy {} your {} at {}.",
						if result.hit.is_some() {
							"hit"
						} else {
							"missed"
						},
						result
							.hit
							.filter(|s| !s.is_empty())
							.map(|s| format!("{:?}", s))
							.unwrap_or_else(|| "ships".to_string()),
						format_args!("({}, {})", result.aim.0, result.aim.1)
					));
					if result.hit.is_some() {
						msgs.push(format!(
							"The enemy {} your {}.",
							if result.sunk.is_some() {
								"sunk"
							} else {
								"failed to sink"
							},
							result
								.sunk
								.filter(|s| !s.is_empty())
								.map(|s| format!("{:?}", s))
								.unwrap_or_else(|| "ship".to_string()),
						))
					}
					msg.write().await.append(&mut msgs);
					*last_result.write().await = Some(result);
				}))
			}
			ui.label("Waiting for the enemy to fire.");
		}
	}
}
