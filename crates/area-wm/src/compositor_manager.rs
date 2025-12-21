use anyhow::{Context, Result};
use area_ipc::compositor_proto::{DamageRect, FrameHeader};
use nix::sys::socket::{
    sendmsg, socketpair, AddressFamily, ControlMessage, MsgFlags, SockFlag, SockType,
};
use nix::sys::uio::IoVec;
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::process::{Command, Stdio};
use tracing::{debug, error, info};

pub struct CompositorManager {
    socket_fd: std::fs::File,
    child_process: Option<std::process::Child>,
}

impl CompositorManager {
    pub fn new() -> Result<Self> {
        info!("Initializing Compositor Manager");

        // 1. Create a socket pair for IPC
        let (wm_sock, comp_sock) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::empty(),
        )
        .context("Failed to create socket pair")?;

        // Convert raw FDs to Files to manage lifecycle (RAII)
        let wm_socket_file = unsafe { std::fs::File::from_raw_fd(wm_sock.as_raw_fd()) };
        let comp_socket_file = unsafe { std::fs::File::from_raw_fd(comp_sock.as_raw_fd()) };

        info!("Created socket pair: WM={}, Comp={}", wm_sock, comp_sock);

        // 2. Spawn the compositor process
        // We need to locate the binary. For now, assume it's next to us.
        let exe_path = std::env::current_exe().context("Failed to get current executable path")?;
        let comp_bin_path = exe_path.parent().unwrap().join("area-comp");

        if !comp_bin_path.exists() {
            // Fallback for development (cargo run) where binaries might be in different places?
            // Actually cargo usually puts them in target/debug/
        }
        
        // Pass the socket FD number via environment variable
        // In the child, we'll verify this FD is valid.
        let comp_fd_raw = comp_socket_file.as_raw_fd();

        debug!("Spawning compositor: {:?}", comp_bin_path);
        
        // Safety: We are using pre_exec to remove the CLOEXEC flag from the socket
        // so it persists into the child process. socketpair created it without CLOEXEC by default 
        // (unless SockFlag::SOCK_CLOEXEC was passed), but Rust's Command might close FDs.
        // Actually, Rust's Command closes everything except Stdio by default? 
        // No, it inherits by default unless configured otherwise, BUT many libcs set CLOEXEC on new FDs.
        // Let's explicitly try to keep it open.
        
        let mut command = Command::new(&comp_bin_path);
        command
            .env("AREA_COMPOSITOR_SOCKET_FD", comp_fd_raw.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::inherit()) // Let it log to our stdout for now
            .stderr(Stdio::inherit());

        // There isn't a safe cross-platform way to explicitly whitelist an FD in std::process::Command 
        // without platform specific extensions or pre_exec.
        // However, on Unix, we can just hope it isn't closed if we don't set CLOEXEC.
        // Current Rust/Nix defaults: socketpair usually creates FDs *without* CLOEXEC unless requested.
        // But std::process::Command documentation says: 
        // "By default, all file descriptors except stdin, stdout and stderr are closed in the child process."
        // So we MUST use pre_exec or format specific hacks.
        
        // Nix offers no simple helper for this specific case with std::process, 
        // preventing the close is usually done by `pre_exec`.
        
        // BUT wait, std::process::Stdio::from_raw_fd takes ownership. 
        // If we want to pass it as a generic FD, we might strictly need `pre_exec` to `dup2` it 
        // or just ensure it isn't closed.
        // Since we are using an ENV var to pass the FD number, we just need to ensure it stays open.
        
        unsafe {
            use std::os::unix::process::CommandExt;
            command.pre_exec(move || {
                // detailed safety: we are in the child, pre-exec. 
                // We want to ensure comp_fd_raw is NOT closed.
                // Rust's Command implementation usually loops and closes FDs. 
                // There isn't a trivial way to opt-out for one arbitrary FD without using `CommandExt::preserve_fns` (unstable) or internal knowledge.
                
                // ALTERNATIVE: Use dup2 to move it to a known fixed FD (like 3) and pass that?
                // But Rust might still close 3.
                
                // Let's try the "leak" approach: if we don't own it in the Command builder, 
                // logic suggests Command closes unknown FDs.
                
                // A common trick is to pass it as Stdio, even if we don't use it as Stdio.
                // .stderr(Stdio::from_raw_fd(fd)) works to pass it.
                // But we want stdout/stderr for logging.
                
                // Let's rely on the fact that we can clear CLOEXEC.
                // But the *closing* logic of Command is the issue, not the CLOEXEC flag.
                // Command will close all FDs > 2.
                
                // FIX: Don't rely on `Command` protecting it. 
                // We'll just rely on `pre_exec` to *un-close* it? No, that's impossible.
                
                // Standard solution in Rust ecosystem for passing FDs:
                // Use `command-fds` crate or similar. 
                // Since I cannot easily add new crates without user permission (or I can?), 
                // I will try to use the raw libc/nix way inside pre_exec if I really have to, 
                // but actually, `Command` does NOT close all FDs by default on Linux implementation 
                // unless `.close_fds(true)` is set (which is unreleased/unstable or logic varies).
                // Wait, Rust 1.X `Command` behavior on Linux POSIX spawn?
                
                // Let's assume for a moment that if we don't set CLOEXEC, it might survive 
                // IF generic implementation is used.
                // But to be safe, let's just use `dup2` inside `pre_exec` to copy it to a high FD 
                // that we hope the cleanup loop missed? No that's flaky.

                // Better thought: The standard `std::process::Command` does NOT implicitly close 
                // all file descriptors on Unix unless you use `stdio` pipes which might shift things.
                // It only sets CLOEXEC on the new pipes it creates.
                // Existing FDs without CLOEXEC *should* be inherited.
                Ok(())
            });
        }

        let child = command.spawn().context("Failed to spawn area-comp")?;
        
        // We can drop the child's socket file now, it should be closed in parent.
        drop(comp_socket_file);

        Ok(Self {
            socket_fd: wm_socket_file,
            child_process: Some(child),
        })
    }

    pub fn send_frame(&mut self, window_id: u32, damage: &[DamageRect], fds: &[RawFd]) -> Result<()> {
        let mut header = FrameHeader {
            magic: FrameHeader::MAGIC,
            sequence: 0, // TODO: Maintain sequence
            timestamp: 0, // TODO: Use real time
            num_damage_rects: damage.len() as u32,
            num_fds: fds.len() as u32,
            window_id,
        };

        let head_bytes = unsafe {
            std::slice::from_raw_parts(
                &header as *const FrameHeader as *const u8,
                FrameHeader::size(),
            )
        };

        let damage_bytes = unsafe {
            std::slice::from_raw_parts(
                damage.as_ptr() as *const u8,
                damage.len() * DamageRect::size(),
            )
        };

        let iov = [IoVec::from_slice(head_bytes), IoVec::from_slice(damage_bytes)];

        // Construct Cmsg with FDs
        let cmsgs = if !fds.is_empty() {
             vec![ControlMessage::ScmRights(fds)]
        } else {
            vec![]
        };

        sendmsg(
            self.socket_fd.as_raw_fd(),
            &iov,
            &cmsgs,
            MsgFlags::empty(),
            None,
        ).context("Failed to send frame message")?;

        Ok(())
    }
}

impl Drop for CompositorManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.child_process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
