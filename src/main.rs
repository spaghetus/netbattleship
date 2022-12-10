use netbattleship::{parse_coord, read_from, write_to, Game, NetMsg, Ship};
use std::io::{stdout, BufRead, Read, Write};
use std::{
	net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream},
	time::Duration,
};
use structopt::StructOpt;

#[derive(StructOpt)]
struct Args {
	/// The IP address of the other computer.
	pub server: SocketAddrV4,
	/// Whether to act as a server.
	#[structopt(short, long)]
	pub serve: bool,
}

fn main() {
	let args = Args::from_args();

	let mut stream = if args.serve {
		println!("Listening on {}", args.server);
		let list = TcpListener::bind(args.server).expect("Failed to listen.");
		list.accept().expect("Failed to receive connection.").0
	} else {
		println!("Connecting to {}", args.server);
		TcpStream::connect_timeout(
			&std::net::SocketAddr::V4(args.server),
			Duration::from_secs(10),
		)
		.expect("Failed to connect.")
	};

	println!("Got connection...");
	let msg = NetMsg::Hello(0);
	write_to(&msg, &mut stream);
	let resp: NetMsg = read_from(&mut stream);
	assert_eq!(msg, resp);
	println!("Got good handshake!");
	let mut game = Game {
		you: !args.serve,
		..Default::default()
	};
	let mut stdin = std::io::stdin().lock().lines().flatten();

	println!("Ship placement phase...");
	{
		for ship_kind in Ship::into_iter() {
			loop {
				println!("{}", String::from(game.clone()));
				print!("Input the position of {:?} (like E5): ", &ship_kind);
				flush();
				let input = stdin.next().expect("Broken pipe");
				if let Some((x, y)) = parse_coord(input) {
					print!("Vertical? (yn): ");
					flush();
					let input = stdin.next().expect("Broken pipe");
					let vertical = input == "y";
					if ship_kind.place(&mut game.board[game.you as usize], (x, y), vertical) {
						break;
					} else {
						println!("Bad placement, try again.");
					}
				}
			}
		}
	}

	write_to(&NetMsg::Finished, &mut stream);
	println!("Done placing ships! Waiting for the other player...");
	assert_eq!(NetMsg::Finished, read_from(&mut stream));
	println!("Other player is finished! Starting the game...");
	loop {
		println!("{}", String::from(game.clone()));
		if game.turn == game.you {
			// Choose a target
			println!("It's your turn!");
			stdout().lock().write_all(&[7]).expect("Broken pipe");
			print!("Input a guess (like E5): ");
			flush();
			let coords = match parse_coord(stdin.next().expect("Broken pipe")) {
				Some(c) => c,
				None => continue,
			};
			// Fire
			write_to(&NetMsg::Fire(coords.0, coords.1), &mut stream);
			// Did we hit?
			let resp = read_from(&mut stream);
			match resp {
				NetMsg::DidHit(false) => {
					println!("Miss...");
					game.board[1 ^ game.you as usize]
						.board
						.insert(coords, Ship::Miss);
				}
				NetMsg::DidHit(true) => {
					println!("Hit!");
					game.board[1 ^ game.you as usize]
						.board
						.insert(coords, Ship::Hit);
				}
				m => panic!("Unexpected {:?}", m),
			}
			// Did we sink?
			let resp = read_from(&mut stream);
			match resp {
				NetMsg::Sunk(Ship::None) => {}
				NetMsg::Sunk(s) => {
					println!("Sunk {:?}!", s);
				}
				m => panic!("Unexpected {:?}", m),
			}
			// Is the game over?
			let resp = read_from(&mut stream);
			match resp {
				NetMsg::Finished => {
					println!("You win!!!");
					break;
				}
				NetMsg::NotFinished => {}
				m => panic!("Unexpected {:?}", m),
			}
		} else {
			// Enemy's turn.
			println!("Waiting for the enemy to aim...");
			let msg = read_from(&mut stream);
			let aim = match msg {
				NetMsg::Fire(x, y) => (x, y),
				m => panic!("Unexpected {:?}", m),
			};
			println!("The enemy fired at {}{}!", (aim.0 + b'A') as char, aim.1);
			// Did the enemy hit the ship?
			let hit = game.board[game.you as usize]
				.board
				.insert(aim, Ship::Hit)
				.filter(|v| !matches!(v, Ship::Hit | Ship::None | Ship::Miss))
				.unwrap_or(Ship::None);
			match hit {
				Ship::None => {
					write_to(&NetMsg::DidHit(false), &mut stream);
					println!("The enemy missed.")
				}
				s => {
					write_to(&NetMsg::DidHit(true), &mut stream);
					println!("The enemy hit your {:?}!", s);
				}
			}
			// Did the enemy sink our ship?
			if !matches!(hit, Ship::None) && !game.board[game.you as usize].contains(hit) {
				println!("The enemy has sank your {:?}!", hit);
				write_to(&NetMsg::Sunk(hit), &mut stream);
			} else {
				write_to(&NetMsg::Sunk(Ship::None), &mut stream);
			}
			// Did we lose?
			if game.board[game.you as usize]
				.board
				.iter()
				.all(|(_, ship)| ship.is_empty())
			{
				println!("You lose!");
				write_to(&NetMsg::Finished, &mut stream);
				break;
			} else {
				write_to(&NetMsg::NotFinished, &mut stream);
			}
		}
		game.turn ^= true;
	}
	stream
		.shutdown(std::net::Shutdown::Both)
		.expect("Bad shutdown");
}

fn flush() {
	stdout().lock().flush().expect("Broken pipe");
}
