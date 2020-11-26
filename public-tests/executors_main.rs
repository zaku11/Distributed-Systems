use assignment_1_solution::{Handler, ModuleRef, System};
use std::borrow::BorrowMut;
use std::time::Duration;

struct PingPong {
    other: Option<ModuleRef<PingPong>>,
    received_msgs: u32,
    first: bool,
    name: &'static str,
}

#[derive(Clone, Debug)]
struct Ball {}

#[derive(Clone, Debug)]
struct Init {
    target: ModuleRef<PingPong>,
}

impl Handler<Init> for PingPong {
    fn handle(&mut self, msg: Init) {
        self.other = Some(msg.target);
        if self.first {
            self.other.as_ref().unwrap().send(Ball {});
        }
    }
}

impl Handler<Ball> for PingPong {
    fn handle(&mut self, _msg: Ball) {
        println!("In {}: received {}\n", self.name, self.received_msgs);

        self.received_msgs += 1;
        if self.received_msgs < 5 {
            self.other.as_ref().unwrap().send(Ball {})
        }
    }
}

fn initialize_system(sys: &mut System) {
    let ping = sys.register_module(PingPong {
        other: None,
        name: "Ping",
        received_msgs: 0,
        first: true,
    });
    let pong = sys.register_module(PingPong {
        other: None,
        name: "Pong",
        received_msgs: 0,
        first: false,
    });

    ping.send(Init {
        target: pong.clone(),
    });
    pong.send(Init { target: ping });
}

fn main() {
    let mut sys = System::new();
    initialize_system(sys.borrow_mut());

    std::thread::sleep(Duration::from_millis(500));
}
