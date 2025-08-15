use crossbeam::channel::{Receiver, Sender, bounded};
use nix::{
    fcntl::{OFlag, open},
    sys::{
        fanotify::{EventFFlags, Fanotify, FanotifyEvent, InitFlags, MarkFlags, MaskFlags},
        stat::Mode,
    },
};
use std::{
    error::Error,
    result::Result::{Err, Ok},
};
use std::{
    fs,
    os::unix::io::AsRawFd,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

// Acceptable file systems for monitoring TODO: make it configurable
const ACCEPTED_FS: &[&str] = &["ext4", "xfs", "btrfs", "vfat"];

/// Discover all monitored mount points from /proc/mounts
fn monitored_mounts() -> Vec<(String, String)> {
    let mut mounts = Vec::new();
    if let Ok(content) = fs::read_to_string("/proc/mounts") {
        for line in content.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 3 {
                let device = fields[0].to_string();
                let mountpoint = fields[1].to_string();
                let fstype = fields[2];

                if ACCEPTED_FS.contains(&fstype) {
                    mounts.push((device, mountpoint));
                }
            }
        }
    }
    mounts
}

/// Convert raw file descriptor to filesystem path
fn fd_to_path(fd: i32) -> std::io::Result<PathBuf> {
    fs::read_link(format!("/proc/self/fd/{}", fd))
}

/// Translate PID to process name
fn pid_to_name(pid: i32) -> String {
    if pid <= 0 {
        // TODO: pid under 0 represents a error or bug
        return "unknown".into();
    }
    fs::read_to_string(format!("/proc/{}/comm", pid))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

/// Convert fanotify mask to a short symbolic string
fn mask_to_code(mask: MaskFlags) -> String {
    use MaskFlags as MF;
    let mut s = String::new();

    // TODO: put all mask flags
    if mask.contains(MF::FAN_OPEN) {
        s.push('O');
    }
    if mask.contains(MF::FAN_ACCESS) {
        s.push('R');
    }
    if mask.contains(MF::FAN_MODIFY) {
        s.push('W');
    }
    if mask.contains(MF::FAN_CLOSE_WRITE) {
        s.push('C');
    }
    if mask.contains(MF::FAN_CLOSE_NOWRITE) {
        s.push('c');
    }
    if mask.contains(MF::FAN_CREATE) {
        s.push('+');
    }
    if mask.contains(MF::FAN_DELETE) {
        s.push('D');
    }
    if mask.contains(MF::FAN_MOVED_FROM) {
        s.push('<');
    }
    if mask.contains(MF::FAN_MOVED_TO) {
        s.push('>');
    }

    if s.is_empty() {
        s.push('?');
    }
    s
}

/// Setup fanotify instance
fn setup_fanotify() -> nix::Result<Fanotify> {
    Fanotify::init(InitFlags::FAN_CLASS_NOTIF, EventFFlags::O_RDONLY)
}

/// Add a fanotify mark to a given mount path
fn mark_mount<P: AsRef<Path>>(fan: &Fanotify, mount_path: P) -> nix::Result<()> {
    let path = mount_path.as_ref();

    let dirfd = open(
        path,
        OFlag::O_PATH | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC,
        Mode::empty(),
    )?;

    // let events = MaskFlags::FAN_OPEN
    // | MaskFlags::FAN_ACCESS
    // | MaskFlags::FAN_MODIFY
    // | MaskFlags::FAN_CLOSE_WRITE
    // | MaskFlags::FAN_CLOSE_NOWRITE
    // | MaskFlags::FAN_EVENT_ON_CHILD
    // | MaskFlags::FAN_CREATE
    // | MaskFlags::FAN_DELETE
    // | MaskFlags::FAN_MOVED_FROM
    // | MaskFlags::FAN_MOVED_TO;

    fan.mark(
        MarkFlags::FAN_MARK_ADD | MarkFlags::FAN_MARK_MOUNT,
        MaskFlags::FAN_OPEN | MaskFlags::FAN_ACCESS | MaskFlags::FAN_EVENT_ON_CHILD,
        &dirfd,
        Some(path),
    )
}

/// Thread: continuously read events from fanotify and send via channel
fn spawn_reader(fan: Fanotify, tx: Sender<FanotifyEvent>) {
    thread::spawn(move || {
        loop {
            match fan.read_events() {
                Ok(events) => {
                    for ev in events {
                        // send only if consumer is alive
                        if tx.send(ev).is_err() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("fanotify read error: {e}");
                    thread::sleep(Duration::from_millis(250));
                }
            }
        }
    });
}

/// Process fanotify events from channel
fn process_events(rx: Receiver<FanotifyEvent>) {
    for ev in rx.iter() {
        let pid = ev.pid();
        let name = pid_to_name(pid);
        let mask = ev.mask();
        let code = mask_to_code(mask);

        if let Some(fd) = ev.fd() {
            let raw_fd = fd.as_raw_fd();
            let path = fd_to_path(raw_fd)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "[unknown]".into());

            println!("{}({}): {:<3} {}", name, pid, code, path);
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let fan = setup_fanotify()?;
    let mounts = monitored_mounts();

    if mounts.is_empty() {
        eprintln!("No suitable mounts found to monitor.");
        return Ok(());
    }

    for (_dev, mount) in &mounts {
        if let Err(e) = mark_mount(&fan, mount) {
            eprintln!("Failed to mark {}: {}", mount, e);
        }
    }

    let (tx, rx) = bounded::<FanotifyEvent>(512);
    spawn_reader(fan, tx);
    process_events(rx);

    Ok(())
}
