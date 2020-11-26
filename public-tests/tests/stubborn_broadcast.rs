use ntest::timeout;
use std::collections::HashSet;
use std::iter::FromIterator;

use assignment_1_solution::{
    build_stubborn_broadcast, SystemBroadcastMessage, SystemMessage, SystemMessageContent,
    SystemMessageHeader,
};
use tests_lib::TestSender;
use uuid::Uuid;

#[test]
#[timeout(300)]
fn broadcast_sends_to_every_process() {
    // given
    let (tx, rx) = crossbeam_channel::unbounded();

    let processes = HashSet::from_iter(vec![Uuid::new_v4(), Uuid::new_v4()]);
    let mut stubborn_broadcast =
        build_stubborn_broadcast(Box::new(TestSender { tx }), processes.clone());

    // when
    stubborn_broadcast.broadcast(SystemBroadcastMessage {
        forwarder_id: Default::default(),
        message: SystemMessage {
            header: SystemMessageHeader {
                message_source_id: Default::default(),
                message_id: Default::default(),
            },
            data: SystemMessageContent { msg: vec![] },
        },
    });
    let target_processes = HashSet::from_iter(vec![rx.recv().unwrap(), rx.recv().unwrap()]);

    // then
    assert_eq!(processes, target_processes);
}
