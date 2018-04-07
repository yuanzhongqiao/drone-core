//! Single-producer, single-consumer queues.

use core::ops::{BitAnd, BitOr, BitOrAssign, BitXorAssign};
use core::sync::atomic::Ordering;
use core::sync::atomic::Ordering::*;
use futures::task::{self, Task};

pub mod oneshot;
pub mod ring;
pub mod unit;

pub(self) trait SpscInner<A, I>
where
  I: Copy
    + Eq
    + BitAnd<Output = I>
    + BitOr<Output = I>
    + BitOrAssign
    + BitXorAssign,
{
  const ZERO: I;
  const RX_LOCK: I;
  const TX_LOCK: I;
  const COMPLETE: I;

  fn state_load(&self, order: Ordering) -> I;

  fn state_exchange(
    &self,
    current: I,
    new: I,
    success: Ordering,
    failure: Ordering,
  ) -> Result<I, I>;

  #[cfg_attr(feature = "clippy", allow(mut_from_ref))]
  unsafe fn rx_task_mut(&self) -> &mut Option<Task>;

  #[cfg_attr(feature = "clippy", allow(mut_from_ref))]
  unsafe fn tx_task_mut(&self) -> &mut Option<Task>;

  #[inline]
  fn update<F, R, E>(
    &self,
    mut old: I,
    success: Ordering,
    failure: Ordering,
    f: F,
  ) -> Result<R, E>
  where
    F: Fn(&mut I) -> Result<R, E>,
  {
    let cas = |old, new| match self.state_exchange(old, new, success, failure) {
      Ok(x) | Err(x) if x == old => true,
      _ => false,
    };
    loop {
      let mut new = old;
      let result = f(&mut new);
      if result.is_err() || cas(old, new) {
        break result;
      }
      old = self.state_load(failure);
    }
  }

  #[inline(always)]
  fn is_canceled(&self) -> bool {
    self.state_load(Relaxed) & Self::COMPLETE != Self::ZERO
  }

  fn poll_cancel(&self) -> Poll<(), ()> {
    self
      .update(
        self.state_load(Relaxed),
        Acquire,
        Relaxed,
        |state| {
          if *state & (Self::COMPLETE | Self::TX_LOCK) != Self::ZERO {
            Err(())
          } else {
            *state |= Self::TX_LOCK;
            Ok(*state)
          }
        },
      )
      .and_then(|state| {
        unsafe { *self.tx_task_mut() = Some(task::current()) };
        self.update(state, Release, Relaxed, |state| {
          *state ^= Self::TX_LOCK;
          Ok(*state)
        })
      })
      .and_then(|state| {
        if state & Self::COMPLETE != Self::ZERO {
          Err(())
        } else {
          Ok(Async::NotReady)
        }
      })
      .or_else(|()| Ok(Async::Ready(())))
  }

  fn close_half(
    &self,
    task_mut: unsafe fn(&Self) -> &mut Option<Task>,
    half_lock: I,
    complete: I,
    success: Ordering,
  ) {
    self
      .update(
        self.state_load(Relaxed),
        success,
        Relaxed,
        |state| {
          if *state & half_lock == Self::ZERO {
            *state |= half_lock | complete;
            Ok(Some(*state))
          } else if *state & complete == Self::ZERO {
            *state |= complete;
            Ok(None)
          } else {
            Err(())
          }
        },
      )
      .ok()
      .and_then(|state| state)
      .map(|state| {
        unsafe { task_mut(self).take().map(|task| task.notify()) };
        self.update(state, Release, Relaxed, |state| {
          *state ^= half_lock;
          Ok::<(), ()>(())
        })
      });
  }

  #[inline(always)]
  fn close_rx(&self) {
    self.close_half(
      Self::tx_task_mut,
      Self::TX_LOCK,
      Self::COMPLETE,
      Acquire,
    );
  }

  fn drop_rx(&self) {
    self
      .update(
        self.state_load(Relaxed),
        Acquire,
        Relaxed,
        |state| {
          let mut mask = Self::ZERO;
          if *state & Self::TX_LOCK == Self::ZERO {
            mask |= Self::TX_LOCK;
          }
          if *state & Self::RX_LOCK == Self::ZERO {
            mask |= Self::RX_LOCK;
          }
          if mask != Self::ZERO {
            *state |= mask | Self::COMPLETE;
            Ok(Some((*state, mask)))
          } else if *state & Self::COMPLETE == Self::ZERO {
            *state |= Self::COMPLETE;
            Ok(None)
          } else {
            Err(())
          }
        },
      )
      .ok()
      .and_then(|x| x)
      .map(|(state, mask)| {
        if mask & Self::RX_LOCK != Self::ZERO {
          unsafe { self.rx_task_mut().take() };
        }
        if mask & Self::TX_LOCK != Self::ZERO {
          unsafe {
            self
              .tx_task_mut()
              .take()
              .map(|task| task.notify());
          }
        }
        self.update(state, Release, Relaxed, |state| {
          *state ^= mask;
          Ok::<(), ()>(())
        })
      });
  }

  #[inline(always)]
  fn drop_tx(&self) {
    self.close_half(
      Self::rx_task_mut,
      Self::RX_LOCK,
      Self::COMPLETE,
      AcqRel,
    );
  }
}
