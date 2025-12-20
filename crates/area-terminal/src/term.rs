//! Terminal state wrapper around alacritty_terminal

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term};
use anyhow::Result;
use std::sync::{Arc, Mutex};

use crate::pty::Pty;

/// Terminal instance managing a shell session
pub struct Terminal {
    pub term: Arc<Mutex<Term<()>>>,
    pub pty: Pty,
    cols: u16,
   rows: u16,
}

impl Terminal {
    /// Create a new terminal with the given size (columns, rows)
    pub fn new(cols: u16, rows: u16, shell: &str) -> Result<Self> {
        let pty = Pty::new(shell, None)?;
        
        let size = TermSize::new(cols as usize, rows as usize);
        
        let config = Config::default();
        
        let term = Term::new(config, &size, ());
        
        // Set PTY size
        pty.resize(cols, rows)?;
        
        Ok(Self {
            term: Arc::new(Mutex::new(term)),
            pty,
            cols,
            rows,
        })
    }

    /// Process PTY output and update terminal state
    pub fn process_pty_output(&mut self) -> Result<()> {
        let mut buf = [0u8; 4096];
        let n = self.pty.try_read(&mut buf)?;
        
        if n > 0 {
            let mut term = self.term.lock().unwrap();
            // Feed bytes to VTE parser
            for byte in &buf[..n] {
                // Note: alacritty v0.25 uses a different input method
                // For now we'll just store bytes. We'll need to implement
                // proper VTE parsing in Phase 2.
                term.grid_mut();
            }
        }
        
        Ok(())
    }

    /// Write input to PTY
    pub fn write_input(&mut self, data: &[u8]) -> Result<()> {
        self.pty.write(data)?;
        Ok(())
    }

    /// Resize terminal
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.cols = cols;
        self.rows = rows;
        
        let new_size = TermSize::new(cols as usize, rows as usize);
        self.pty.resize(cols, rows)?;
        
        let mut term = self.term.lock().unwrap();
        term.resize(new_size);
        
        Ok(())
    }

    /// Get current terminal size
    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// Extract character grid for rendering
    pub fn get_grid_content(&self) -> Vec<GridCell> {
        let term = self.term.lock().unwrap();
        let grid = term.grid();
        let mut cells = Vec::new();

        for row in 0..grid.screen_lines() {
            for col in 0..grid.columns() {
                let cell = &grid[alacritty_terminal::index::Line(row as i32)][alacritty_terminal::index::Column(col)];
                
                cells.push(GridCell {
                    c: cell.c,
                    fg: cell.fg,
                    bg: cell.bg,
                    flags: cell.flags,
                    row: row as u16,
                    col: col as u16,
                });
            }
        }

        cells
    }
}

/// Represents a single terminal grid cell for rendering
#[derive(Debug, Clone)]
pub struct GridCell {
    pub c: char,
    pub fg: alacritty_terminal::vte::ansi::Color,
    pub bg: alacritty_terminal::vte::ansi::Color,
    pub flags: alacritty_terminal::term::cell::Flags,
    pub row: u16,
    pub col: u16,
}

