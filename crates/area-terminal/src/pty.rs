//! PTY (Pseudo-Terminal) management for spawning and controlling shell processes

use anyhow::{Context, Result};
use std::os::unix::io::RawFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

/// Represents a PTY (pseudo-terminal) instance
pub struct Pty {
    pub master_fd: RawFd,
    pub child: Child,
}

impl Pty {
    /// Spawn a new shell in a PTY
    pub fn new(shell: &str, working_dir: Option<&str>) -> Result<Self> {
        // Open PTY master
        let master_fd = unsafe {
            let fd = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
            if fd < 0 {
                return Err(anyhow::anyhow!("Failed to open PTY master"));
            }
            
            if libc::grantpt(fd) < 0 || libc::unlockpt(fd) < 0 {
                libc::close(fd);
                return Err(anyhow::anyhow!("Failed to grant/unlock PTY"));
            }
            
            fd
        };

        // Get PTY slave name
        let slave_name = unsafe {
            let name_ptr = libc::ptsname(master_fd);
            if name_ptr.is_null() {
                libc::close(master_fd);
                return Err(anyhow::anyhow!("Failed to get PTY slave name"));
            }
            std::ffi::CStr::from_ptr(name_ptr)
                .to_string_lossy()
                .to_string()
        };

        // Spawn child process
        let mut cmd = Command::new(shell);
        
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        // Set up child to use PTY slave
        unsafe {
            cmd.pre_exec(move || {
                // Open slave
                let slave_fd = libc::open(
                    slave_name.as_ptr() as *const libc::c_char,
                    libc::O_RDWR,
                );
                if slave_fd < 0 {
                    return Err(std::io::Error::last_os_error());
                }

                // Create new session
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }

                // Set controlling terminal
                if libc::ioctl(slave_fd, libc::TIOCSCTTY, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }

                // Dup slave to stdin/stdout/stderr
                libc::dup2(slave_fd, 0);
                libc::dup2(slave_fd, 1);
                libc::dup2(slave_fd, 2);

                if slave_fd > 2 {
                    libc::close(slave_fd);
                }

                Ok(())
            });
        }

        let child = cmd.spawn().context("Failed to spawn shell")?;

        Ok(Self { master_fd, child })
    }

    /// Read from PTY (non-blocking)
    pub fn try_read(&self, buf: &mut [u8]) -> Result<usize> {
        unsafe {
            // Set non-blocking
            let flags = libc::fcntl(self.master_fd, libc::F_GETFL, 0);
            libc::fcntl(self.master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);

            let n = libc::read(
                self.master_fd,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
            );

            if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    return Ok(0);
                }
                return Err(err.into());
            }

            Ok(n as usize)
        }
    }

    /// Write to PTY
    pub fn write(&self, data: &[u8]) -> Result<usize> {
        unsafe {
            let n = libc::write(
                self.master_fd,
                data.as_ptr() as *const libc::c_void,
                data.len(),
            );

            if n < 0 {
                return Err(std::io::Error::last_os_error().into());
            }

            Ok(n as usize)
        }
    }

    /// Resize PTY window
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let winsize = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        unsafe {
            if libc::ioctl(self.master_fd, libc::TIOCSWINSZ, &winsize) < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }

        Ok(())
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.master_fd);
        }
        let _ = self.child.kill();
    }
}
