use assignment_1_solution::{
    build_reliable_broadcast, PlainSender, PlainSenderMessage, StubbornBroadcast,
    StubbornBroadcastModule, System, SystemAcknowledgmentMessage, SystemBroadcastMessage,
    SystemMessageContent, SystemMessageHeader,
};
use crossbeam_channel::{unbounded, Sender};
use ntest::timeout;
use tests_lib::RamStorage;
use uuid::Uuid;

#[test]
#[timeout(300)]
fn reliable_broadcast_uses_stubborn_broadcast() {
    // given
    let (tx, rx) = unbounded();
    let mut system = System::new();
    let sbeb = system.register_module(StubbornBroadcastModule::new(Box::new(
        TestStubbornBroadcast { tx },
    )));
    let storage = Box::new(RamStorage::new());
    let ident = Uuid::new_v4();
    let mut reliable_broadcast =
        build_reliable_broadcast(sbeb, storage, ident, 2, Box::new(|_data| {}));

    // when
    reliable_broadcast.broadcast(SystemMessageContent { msg: vec![] });

    // then
    assert_eq!(rx.recv().unwrap(), ident);
}

struct TestStubbornBroadcast {
    tx: Sender<Uuid>,
}

impl StubbornBroadcast for TestStubbornBroadcast {
    fn broadcast(&mut self, msg: SystemBroadcastMessage) {
        self.tx.send(msg.forwarder_id).unwrap();
    }

    fn receive_acknowledgment(&mut self, _proc: Uuid, _msg: SystemMessageHeader) {}

    fn send_acknowledgment(&mut self, _proc: Uuid, _msg: SystemAcknowledgmentMessage) {}

    fn tick(&mut self) {}
}

struct NothingSender {}

impl PlainSender for NothingSender {
    fn send_to(&self, _uuid: &Uuid, _msg: PlainSenderMessage) {}
}
