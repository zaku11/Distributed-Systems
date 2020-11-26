use assignment_1_solution::{setup_system, Configuration, System, SystemMessageContent};
use crossbeam_channel::unbounded;
use ntest::timeout;
use std::collections::HashSet;
use std::iter::FromIterator;
use std::time::{Duration, Instant};
use tests_lib::{RamStorage, TestSender};
use uuid::Uuid;

#[test]
#[timeout(2000)]
fn system_setup_makes_stubborn_broadcast_receive_ticks() {
    // given
    let (tx, rx) = unbounded();
    let sender = TestSender { tx };
    let mut system = System::new();
    let config = Configuration {
        self_process_identifier: Default::default(),
        processes: HashSet::from_iter(vec![Uuid::new_v4()]),
        stable_storage: Box::new(RamStorage::new()),
        sender: Box::new(sender),
        retransmission_delay: Duration::from_millis(500),
        delivered_callback: Box::new(|_data| {}),
    };

    // when
    let reliable_broadcast = setup_system(&mut system, config);
    reliable_broadcast.send(SystemMessageContent { msg: vec![] });

    // then
    rx.recv().unwrap();
    let first_send = Instant::now();
    rx.recv().unwrap();
    let duration_millis = Instant::now().duration_since(first_send).as_millis();
    assert!(duration_millis >= 400 && duration_millis <= 600)
}
