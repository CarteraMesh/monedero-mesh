use std::{
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
  time::Duration,
};

use anyhow::Result;
use crossbeam::channel::Sender;
use ratatui::crossterm::event::{poll, read, Event as CEvent, KeyEvent, KeyEventKind};

use crate::{
  message::{Msg, UserEvent},
  panic_set_hook,
};

pub struct UserInput {
  tx: Sender<Msg>,
  is_terminated: Arc<AtomicBool>,
}

impl UserInput {
  pub fn new(tx: Sender<Msg>, is_terminated: Arc<AtomicBool>) -> Self {
    Self { tx, is_terminated }
  }

  pub fn start(&self) -> Result<()> {
    tracing::info!("user_input start");

    let ret = self.poll();

    self.is_terminated.store(true, Ordering::Relaxed);

    tracing::info!("user_input end");

    ret
  }

  pub fn set_panic_hook(&self) {
    let is_terminated = self.is_terminated.clone();

    panic_set_hook!({
            is_terminated.store(true, Ordering::Relaxed);
        });
  }

  fn poll(&self) -> Result<()> {
    while !self.is_terminated.load(Ordering::Relaxed) {
      if let Ok(true) = poll(Duration::from_secs(1)) {
        let ev = read()?;

        tracing::debug!("{:?}", ev);

        match ev {
          CEvent::Key(ev) => {
            if let KeyEvent {
              kind: KeyEventKind::Press | KeyEventKind::Repeat,
              ..
            } = ev
            {
              self.tx.send(Msg::User(UserEvent::Key(ev)))?
            }
          }
          CEvent::Mouse(ev) => self.tx.send(Msg::User(UserEvent::Mouse(ev)))?,
          CEvent::Resize(..) => {}
          CEvent::FocusGained => self.tx.send(UserEvent::FocusGained.into())?,
          CEvent::FocusLost => self.tx.send(UserEvent::FocusLost.into())?,
          CEvent::Paste(_) => {}
        }
      }
    }

    Ok(())
  }
}
