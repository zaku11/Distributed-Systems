use assignment_1_solution::{Handler, System};
use crossbeam_channel::{unbounded, Sender};
use ntest::timeout;

#[test]
#[timeout(300)]
fn counter_receives_messages() {
    let mut system = System::new();
    let (tx, rx) = unbounded();

    let ctr_ref = system.register_module(Counter::new(tx));

    for i in 1..3 {
        ctr_ref.send(Inc {});
        assert_eq!(i, rx.recv().unwrap());
    }
}

struct Counter {
    num: u64,
    tx: Sender<u64>,
}

impl Counter {
    fn new(tx: Sender<u64>) -> Self {
        Counter { num: 0, tx }
    }
}

#[derive(Debug, Clone)]
struct Inc {}

impl Handler<Inc> for Counter {
    fn handle(&mut self, _msg: Inc) {
        self.num += 1;
        self.tx.send(self.num).unwrap();
    }
}
