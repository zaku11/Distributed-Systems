use assignment_1_solution::{
    build_stable_storage, setup_system, Configuration, ModuleRef, PlainSender, PlainSenderMessage,
    ReliableBroadcastModule, System, SystemMessageContent,
};
use simple_logger::SimpleLogger;
use std::collections::{HashMap, HashSet};
use std::ops::DerefMut;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

type SingleProcessSenderData = HashMap<Uuid, ModuleRef<ReliableBroadcastModule>>;

fn main() {
    SimpleLogger::new()
        .with_level(log::Level::Info.to_level_filter())
        .init()
        .unwrap();
    let tempdir_path = tempfile::tempdir().unwrap();
    let ident1 = Uuid::new_v4();
    let ident2 = Uuid::new_v4();
    let idents: HashSet<Uuid> = vec![ident1, ident2].into_iter().collect();
    let sender = SingleProcessSender::new();
    let mut system = System::new();

    let reg1 = setup_system(
        &mut system,
        Configuration {
            self_process_identifier: ident1,
            processes: idents.clone(),
            stable_storage: build_stable_storage(tempdir_path.path().join("a")),
            sender: sender.duplicate(),
            retransmission_delay: Duration::from_millis(100),
            delivered_callback: Box::new(|msg| {
                log::info!(
                    "Header: {:?}, content: {}",
                    msg.header,
                    std::string::String::from_utf8(msg.data.msg).unwrap()
                );
            }),
        },
    );
    let reg2 = setup_system(
        &mut system,
        Configuration {
            self_process_identifier: ident2,
            processes: idents,
            stable_storage: build_stable_storage(tempdir_path.path().join("b")),
            sender: sender.duplicate(),
            retransmission_delay: Duration::from_millis(100),
            delivered_callback: Box::new(|msg| {
                log::info!(
                    "Header: {:?}, content: {}",
                    msg.header,
                    std::string::String::from_utf8(msg.data.msg).unwrap()
                );
            }),
        },
    );

    {
        let mut data = sender.processes.lock().unwrap();
        data.insert(ident1, reg1.clone());
        data.insert(ident2, reg2);
    }

    loop {
        reg1.send(SystemMessageContent {
            msg: "Hello world!".to_string().into_bytes(),
        });
        std::thread::sleep(Duration::from_secs(5));
    }
}

#[derive(Clone)]
pub struct SingleProcessSender {
    pub processes: Arc<Mutex<SingleProcessSenderData>>,
}

impl SingleProcessSender {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn insert(&self, proc: Uuid, module_ref: ModuleRef<ReliableBroadcastModule>) {
        self.processes.lock().unwrap().insert(proc, module_ref);
    }

    pub fn duplicate(&self) -> Box<dyn PlainSender> {
        Box::new(self.clone())
    }
}

impl Default for SingleProcessSender {
    fn default() -> Self {
        Self::new()
    }
}

impl PlainSender for SingleProcessSender {
    fn send_to(&self, uuid: &Uuid, msg: PlainSenderMessage) {
        let mut lock = self.processes.lock().unwrap();
        let data = lock.deref_mut();

        if let Some(reliable_ref) = data.get_mut(uuid) {
            match msg {
                PlainSenderMessage::Broadcast(broadcast) => reliable_ref.send(broadcast),
                PlainSenderMessage::Acknowledge(acknowledgment) => {
                    reliable_ref.send(acknowledgment)
                }
            };
        }
    }
}
