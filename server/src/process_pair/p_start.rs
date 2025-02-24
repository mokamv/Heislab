use std::env::current_exe;
use std::process::{Child, Command, Stdio};

pub(super) enum ProcessType {
    MAIN,
    BACKUP(u32)
}

pub(super) fn launch_process(p_type: ProcessType) -> Child {
    let p_args= match p_type {
        ProcessType::MAIN => "--main".to_string(),
        ProcessType::BACKUP(counter) => format!("--backup {}", counter)
    };

    let mut cmd = Command::new("bash");
    cmd.arg("-c");
    cmd.arg(
        format!(
            "{:?} {}",
            current_exe().unwrap(),
            p_args
        )
    );

    match cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn() {
        Err(why) => panic!("couldn't spawn backup: {}", why),
        Ok(child) => child
    }
}