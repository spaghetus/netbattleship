//! Speech-friendly terminal program for netbattleship
//! Has the same arguments as the normal CLI

use netbattleship::{
	flow::GameFlow,
	ui::{self, parse_coord},
	Phase,
};
use rustyline::Editor;
use std::{net::SocketAddrV4, time::Duration};
use structopt::StructOpt;
use tokio::{
	io::{stdout, AsyncWriteExt},
	task::yield_now,
};
use tts::Tts;

#[derive(StructOpt)]
struct Args {
	/// The IP address of the other computer.
	pub server: SocketAddrV4,
	/// Whether to act as a server.
	#[structopt(short, long)]
	pub serve: bool,
	/// Whether to call speech apis directly
	#[structopt(short = "S", long)]
	pub speak: bool,
	/// How fast to talk
	#[structopt(short = "p", long, default_value = "1.0")]
	pub speed: f32,
}

async fn put(tts: &mut Option<Tts>, text: &str) {
	println!("{}", text);
	stdout().flush().await.expect("Broken pipe");
	if let Some(tts) = tts {
		if let Err(e) = tts.speak(text, true) {
			eprintln!("Speech failed with {}.", e)
		}
	}
}

async fn wait_for_tts(tts: &mut Option<Tts>) {
	let mut counter = 0;
	while let Some(Ok(false)) = tts.as_mut().map(|v| v.is_speaking()) {
		tokio::time::sleep(Duration::from_millis(1)).await;
		if counter > 100 {
			break;
		}
		counter += 1;
	}
	while let Some(Ok(true)) = tts.as_mut().map(|v| v.is_speaking()) {
		yield_now().await;
	}
}

#[tokio::main]
async fn main() {
	let args = Args::from_args();
	let mut tts = if args.speak {
		let tts = tts::Tts::default();
		if let Err(e) = &tts {
			eprintln!(
				"Failed to connect to system TTS because {}, falling back to print line.",
				e
			);
		}
		tts.map(|mut tts| {
			tts.set_rate(args.speed * tts.normal_rate())
				.expect("Failed to set speech rate");
			tts
		})
		.ok()
	} else {
		None
	};
	let mut rl = Editor::<()>::new().expect("Failed to open prompt");

	put(&mut tts, "Connecting...").await;
	let game = GameFlow::new(std::net::SocketAddr::V4(args.server), args.serve)
		.await
		.expect("Failed to connect");

	put(
		&mut tts,
		"This is the speech version of net battleship. You can type 'help' in any mode to learn which commands are available.",
	)
	.await;

	loop {
		wait_for_tts(&mut tts).await;
		if game.my_turn().await || game.phase().await != Phase::Playing {
			put(
				&mut tts,
				&match game.state.read().await.phase {
					netbattleship::Phase::Connecting => "Connecting...".to_string(),
					netbattleship::Phase::Placing(s) => format!("Placing {:?}:", s),
					netbattleship::Phase::Playing => "Your turn.".to_string(),
					netbattleship::Phase::Done(_) => "Done!".to_string(),
				},
			)
			.await;
			let readline = match rl.readline("") {
				Ok(s) => s,
				Err(e) => panic!("Reading failed with {}", e),
			};
			match readline.to_lowercase().as_str() {
				"help" => {
					put(
							&mut tts,
							&match game.state.read().await.phase {
								netbattleship::Phase::Connecting => {
									"Commands are unavailable while connecting.".to_string()
								}
								netbattleship::Phase::Placing(_) => {
									[
										"When placing a ship, you can take the following actions:",
										"1. Query the board, by typing the letter Q, followed by a letter from A to J and a number from 0 to 9.",
										"When querying the board, use a lowercase Q to query the enemy's board, and an uppercase Q to query your own.",
										"2. Place a ship, by typing the letter P, followed by a letter from A to J, a number from 0 to 9, and optionally the letter V.",
										"If V is omitted, the ship will be placed pointing right, in the increasing number direction.",
										"If V is included, the ship will be placed pointing downwards, in the increasing letter direction.",
										"3. Do nothing, to hear the prompt again, by pressing enter without typing anything."
									].join("\n")
								}
								netbattleship::Phase::Playing => [
									"When playing, you can only input commands when it is your turn.",
									"Any inputs during the enemy's turn will be buffered until your turn.",
									"When it is your turn, you can take the following actions:",
									"1. Query the board, by typing the letter Q, followed by a letter from A to J and a number from 0 to 9.",
									"When querying the board, use a lowercase Q to query the enemy's board, and an uppercase Q to query your own.",
									"2. Fire, by pressing the letter F, followed by a letter from A to J and a number from 0 to 9.",
									"3. Do nothing, to hear the prompt again, by pressing enter without typing anything."
								].join("\n"),
								netbattleship::Phase::Done(_) => [
									"After the game ends, you can take the following actions:",
									"1. Query the board, by typing the letter Q, followed by a letter from A to J and a number from 0 to 9.",
									"When querying the board, use a lowercase Q to query the enemy's board, and an uppercase Q to query your own.",
									"2. Exit the game, by pressing control + c."
								].join("\n"),
							},
						)
						.await;
				}
				c if c.starts_with('p') => {
					if let Phase::Placing(ship) = game.phase().await {
						let coords = ui::parse_coord(&c[1..3]);
						match coords {
							Some(pos) => {
								match game.place_ship(ship, pos, c.get(3..4) == Some("v")).await {
									Ok(()) => put(&mut tts, "OK").await,
									Err(e) => match e {
										netbattleship::flow::GameFlowError::InvalidPlacement => {
											put(&mut tts, "Placement out of bounds.").await
										}
										e => panic!("{}", e),
									},
								}
							}
							None => put(&mut tts, "Bad coordinates").await,
						}
					} else {
						put(&mut tts, "Cannot place a ship in this phase.").await;
					}
				}
				c if c.starts_with('q') => {
					if let Phase::Placing(_) | Phase::Playing | Phase::Done(_) = game.phase().await
					{
						let query_self = readline
							.chars()
							.next()
							.map(|c| c.is_uppercase())
							.unwrap_or(false);
						let board = game.board(!query_self).await;
						let coords = parse_coord(&c[1..3]);
						match coords {
							Some(pos) => {
								put(
									&mut tts,
									match board
										.board
										.get(&pos)
										.unwrap_or(&netbattleship::ship::Ship::None)
									{
										netbattleship::ship::Ship::None => "Empty.",
										netbattleship::ship::Ship::Miss => "Missed shot.",
										netbattleship::ship::Ship::Hit => "True shot.",
										netbattleship::ship::Ship::Carrier => "Aircraft carrier.",
										netbattleship::ship::Ship::Battleship => "Battleship.",
										netbattleship::ship::Ship::Cruiser => "Cruiser.",
										netbattleship::ship::Ship::Submarine => "Submarine.",
										netbattleship::ship::Ship::Destroyer => "Destroyer.",
									},
								)
								.await
							}
							None => put(&mut tts, "Bad coordinates").await,
						}
					} else {
						put(&mut tts, "Cannot query the board in this phase.").await;
					}
				}
				c if c.starts_with('f') => {
					if let Phase::Playing = game.phase().await {
						if game.my_turn().await {
							let coords = ui::parse_coord(&c[1..3]);
							match coords {
								Some(pos) => match game.fire(pos).await {
									Ok(result) => {
										if result.hit.is_some() {
											put(&mut tts, "Your shot hit the enemy.").await;
											wait_for_tts(&mut tts).await;
										} else {
											put(&mut tts, "Your shot hit the waves.").await;
											wait_for_tts(&mut tts).await;
										}
										if let Some(ship) = result.sunk {
											put(
												&mut tts,
												&format!("You sunk the enemy {:?}!", ship),
											)
											.await;
											wait_for_tts(&mut tts).await;
										}
										if result.won {
											put(&mut tts, "You won the game!").await;
											break;
										}
									}
									Err(e) => match e {
										netbattleship::flow::GameFlowError::InvalidPlacement => {
											put(&mut tts, "Placement out of bounds.").await
										}
										e => panic!("{}", e),
									},
								},
								None => put(&mut tts, "Bad coordinates").await,
							}
						}
					} else {
						put(&mut tts, "Cannot place a ship in this phase.").await;
					}
				}

				_ => put(&mut tts, "Unknown command.").await,
			}
		} else {
			put(&mut tts, "Enemy turn.").await;
			let result = game.receive().await.unwrap();
			if let Some(ship) = result.hit {
				put(&mut tts, &format!("The enemy's shot hit your {:?}.", ship)).await;
				wait_for_tts(&mut tts).await;
			} else {
				put(&mut tts, "The enemy's shot hit the waves.").await;
				wait_for_tts(&mut tts).await;
			}
			if let Some(ship) = result.sunk {
				put(&mut tts, &format!("The enemy sunk your {:?}.", ship)).await;
				wait_for_tts(&mut tts).await;
			}
			if result.won {
				put(&mut tts, "You lost the game...").await;
			}
		}
	}
}
