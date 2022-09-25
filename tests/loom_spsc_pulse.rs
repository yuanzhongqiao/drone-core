#[macro_use]
mod loom_spsc;

use std::pin::Pin;
use std::task::Poll;

use drone_core::sync::new_spsc::pulse::{channel, TryNextError};
use futures::prelude::*;
use futures::stream::FusedStream;

use self::loom_spsc::*;

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_drop() {
    loom::model(|| {
        let (tx, rx) = channel::<CheckDrop>();
        let tx = loom::thread::spawn(move || drop(tx));
        let rx = loom::thread::spawn(move || drop(rx));
        tx.join().unwrap();
        rx.join().unwrap();
    });
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_try_next() {
    loom::model(|| {
        let (tx, mut rx) = channel::<CheckDrop>();
        let tx = loom::thread::spawn(move || drop(tx));
        let rx = loom::thread::spawn(move || match rx.try_next() {
            Err(_) => {}
            value => panic!("{value:#?} variant is incorrect"),
        });
        tx.join().unwrap();
        rx.join().unwrap();
    });
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_next() {
    let rx_states = statemap![
        0 => [0, 10100],
        3 => [101],
        6 => [101, 10100],
    ];
    loom::model(move || {
        async_context!(rx_counter, rx_waker, rx_cx);
        let (tx, mut rx) = channel::<CheckDrop>();
        let tx = loom::thread::spawn(move || drop(tx));
        let rx = loom::thread::spawn(move || match Pin::new(&mut rx).poll_next(&mut rx_cx) {
            Poll::Ready(None) => {
                assert!(rx.is_terminated());
                0
            }
            Poll::Ready(Some(Ok(_))) => 1,
            Poll::Ready(Some(Err(_))) => 2,
            Poll::Pending => match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                Poll::Ready(None) => {
                    assert!(rx.is_terminated());
                    3
                }
                Poll::Ready(Some(Ok(_))) => 4,
                Poll::Ready(Some(Err(_))) => 5,
                Poll::Pending => 6,
            },
        });
        tx.join().unwrap();
        statemap_put(rx_states, rx_counter, rx.join().unwrap());
    });
    statemap_check_exhaustive(rx_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_cancellation() {
    let tx_states = statemap![
        0 => [0, 10100],
        1 => [101],
        2 => [101, 10100],
    ];
    loom::model(move || {
        async_context!(tx_counter, tx_waker, tx_cx);
        let (mut tx, rx) = channel::<CheckDrop>();
        let rx = loom::thread::spawn(move || drop(rx));
        let tx =
            loom::thread::spawn(move || match Pin::new(&mut tx.cancellation()).poll(&mut tx_cx) {
                Poll::Ready(()) => {
                    assert!(tx.is_canceled());
                    0
                }
                Poll::Pending => match Pin::new(&mut tx.cancellation()).poll(&mut tx_cx) {
                    Poll::Ready(()) => {
                        assert!(tx.is_canceled());
                        1
                    }
                    Poll::Pending => 2,
                },
            });
        rx.join().unwrap();
        statemap_put(tx_states, tx_counter, tx.join().unwrap());
    });
    statemap_check_exhaustive(tx_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_cancellation_persistent() {
    let tx_states = statemap![
        0 => [0, 10100],
        1 => [101],
        2 => [101],
    ];
    loom::model(move || {
        async_context!(tx_counter, tx_waker, tx_cx);
        let (mut tx, rx) = channel::<CheckDrop>();
        let rx = loom::thread::spawn(move || drop(rx));
        let tx = loom::thread::spawn(move || {
            let value = match Pin::new(&mut tx.cancellation()).poll(&mut tx_cx) {
                Poll::Ready(()) => {
                    assert!(tx.is_canceled());
                    0
                }
                Poll::Pending => match Pin::new(&mut tx.cancellation()).poll(&mut tx_cx) {
                    Poll::Ready(()) => {
                        assert!(tx.is_canceled());
                        1
                    }
                    Poll::Pending => 2,
                },
            };
            (tx, tx_cx, value)
        });
        rx.join().unwrap();
        let (mut tx, mut tx_cx, mut tx_value) = tx.join().unwrap();
        if tx_value == 2 {
            tx_value = match Pin::new(&mut tx.cancellation()).poll(&mut tx_cx) {
                Poll::Ready(()) => {
                    assert!(tx.is_canceled());
                    2
                }
                Poll::Pending => 3,
            };
        }
        statemap_put(tx_states, tx_counter, tx_value);
    });
    statemap_check_exhaustive(tx_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_close_cancellation() {
    let tx_states = statemap![
        0 => [0, 10100],
        1 => [101],
        2 => [101, 10100],
    ];
    loom::model(move || {
        async_context!(tx_counter, tx_waker, tx_cx);
        let (mut tx, mut rx) = channel::<CheckDrop>();
        let rx = loom::thread::spawn(move || rx.close());
        let tx =
            loom::thread::spawn(move || match Pin::new(&mut tx.cancellation()).poll(&mut tx_cx) {
                Poll::Ready(()) => {
                    assert!(tx.is_canceled());
                    0
                }
                Poll::Pending => match Pin::new(&mut tx.cancellation()).poll(&mut tx_cx) {
                    Poll::Ready(()) => {
                        assert!(tx.is_canceled());
                        1
                    }
                    Poll::Pending => 2,
                },
            });
        rx.join().unwrap();
        statemap_put(tx_states, tx_counter, tx.join().unwrap());
    });
    statemap_check_exhaustive(tx_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_close_cancellation_persistent() {
    let tx_states = statemap![
        0 => [0, 10100],
        1 => [101],
        2 => [101],
    ];
    loom::model(move || {
        async_context!(tx_counter, tx_waker, tx_cx);
        let (mut tx, mut rx) = channel::<CheckDrop>();
        let rx = loom::thread::spawn(move || {
            rx.close();
            rx
        });
        let tx = loom::thread::spawn(move || {
            let value = match Pin::new(&mut tx.cancellation()).poll(&mut tx_cx) {
                Poll::Ready(()) => {
                    assert!(tx.is_canceled());
                    0
                }
                Poll::Pending => match Pin::new(&mut tx.cancellation()).poll(&mut tx_cx) {
                    Poll::Ready(()) => {
                        assert!(tx.is_canceled());
                        1
                    }
                    Poll::Pending => 2,
                },
            };
            (tx, tx_cx, value)
        });
        let rx = rx.join().unwrap();
        let (mut tx, mut tx_cx, mut tx_value) = tx.join().unwrap();
        if tx_value == 2 {
            tx_value = match Pin::new(&mut tx.cancellation()).poll(&mut tx_cx) {
                Poll::Ready(()) => {
                    assert!(tx.is_canceled());
                    2
                }
                Poll::Pending => 3,
            };
        }
        drop(rx);
        statemap_put(tx_states, tx_counter, tx_value);
    });
    statemap_check_exhaustive(tx_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_send_err_next() {
    let rx_states = statemap![
        12 => [0, 10100],
        15 => [101],
        16 => [101],
        26 => [10100],
    ];
    let data_states = statemap![
        12 => [10],
        15 => [10],
        16 => [1],
        26 => [10],
    ];
    loom::model(move || {
        async_context!(rx_counter, rx_waker, rx_cx);
        let (tx, mut rx) = channel::<CheckDrop>();
        check_drop!(data_counter, data, 314);
        let tx = loom::thread::spawn(move || match tx.send_err(data) {
            Ok(()) => 10,
            Err(value) => {
                assert_eq!(value.get(10), 314);
                20
            }
        });
        let rx = loom::thread::spawn(move || match Pin::new(&mut rx).poll_next(&mut rx_cx) {
            Poll::Ready(None) => 0,
            Poll::Ready(Some(Ok(_))) => 1,
            Poll::Ready(Some(Err(value))) => {
                assert!(rx.is_terminated());
                assert_eq!(value.get(10), 314);
                2
            }
            Poll::Pending => match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                Poll::Ready(None) => 3,
                Poll::Ready(Some(Ok(_))) => 4,
                Poll::Ready(Some(Err(value))) => {
                    assert!(rx.is_terminated());
                    assert_eq!(value.get(10), 314);
                    5
                }
                Poll::Pending => 6,
            },
        });
        let key = tx.join().unwrap() + rx.join().unwrap();
        statemap_put(rx_states, rx_counter, key);
        statemap_put(data_states, data_counter, key);
    });
    statemap_check_exhaustive(rx_states);
    statemap_check_exhaustive(data_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_send_err_next_persistent() {
    let rx_states = statemap![
        12 => [0, 10100],
        15 => [101],
        18 => [101],
    ];
    let data_states = statemap![
        12 => [10],
        15 => [10],
        18 => [10],
    ];
    loom::model(move || {
        async_context!(rx_counter, rx_waker, rx_cx);
        let (tx, mut rx) = channel::<CheckDrop>();
        check_drop!(data_counter, data, 314);
        let tx = loom::thread::spawn(move || match tx.send_err(data) {
            Ok(()) => 10,
            Err(_) => 20,
        });
        let rx = loom::thread::spawn(move || {
            let value = match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                Poll::Ready(None) => 0,
                Poll::Ready(Some(Ok(_))) => 1,
                Poll::Ready(Some(Err(value))) => {
                    assert!(rx.is_terminated());
                    assert_eq!(value.get(10), 314);
                    2
                }
                Poll::Pending => match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                    Poll::Ready(None) => 3,
                    Poll::Ready(Some(Ok(_))) => 4,
                    Poll::Ready(Some(Err(value))) => {
                        assert!(rx.is_terminated());
                        assert_eq!(value.get(10), 314);
                        5
                    }
                    Poll::Pending => 6,
                },
            };
            (rx, rx_cx, value)
        });
        let tx_value = tx.join().unwrap();
        let (mut rx, mut rx_cx, mut rx_value) = rx.join().unwrap();
        if rx_value == 6 {
            rx_value = match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                Poll::Ready(None) => 6,
                Poll::Ready(Some(Ok(_))) => 7,
                Poll::Ready(Some(Err(value))) => {
                    assert!(rx.is_terminated());
                    assert_eq!(value.get(10), 314);
                    8
                }
                Poll::Pending => 9,
            };
        }
        let key = tx_value + rx_value;
        statemap_put(rx_states, rx_counter, key);
        statemap_put(data_states, data_counter, key);
    });
    statemap_check_exhaustive(rx_states);
    statemap_check_exhaustive(data_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_send_send_next_persistent() {
    let rx_states = statemap![
        1273 => [102],
        1230 => [0, 101, 102, 10100],
        1400 => [0, 101, 10100],
        1410 => [0, 101, 10100],
        1470 => [101],
        1723 => [103],
        1740 => [103],
        1774 => [103],
    ];
    loom::model(move || {
        async_context!(rx_counter, rx_waker, rx_cx);
        let (mut tx, mut rx) = channel::<CheckDrop>();
        let tx = loom::thread::spawn(move || match tx.send(11) {
            Ok(()) => match tx.send(13) {
                Ok(()) => 1000,
                Err(_) => 2000,
            },
            Err(_) => 3000,
        });
        let rx = loom::thread::spawn(move || {
            let mut value = match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                Poll::Ready(None) => 100,
                Poll::Ready(Some(Ok(value))) if value.get() == 11 => 200,
                Poll::Ready(Some(Ok(value))) if value.get() == 13 => 300,
                Poll::Ready(Some(Ok(value))) if value.get() == 24 => 400,
                Poll::Ready(Some(Ok(_))) => 500,
                Poll::Ready(Some(Err(_))) => 600,
                Poll::Pending => 700,
            };
            if !rx.is_terminated() {
                value += match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                    Poll::Ready(None) => 10,
                    Poll::Ready(Some(Ok(value))) if value.get() == 11 => 20,
                    Poll::Ready(Some(Ok(value))) if value.get() == 13 => 30,
                    Poll::Ready(Some(Ok(value))) if value.get() == 24 => 40,
                    Poll::Ready(Some(Ok(_))) => 50,
                    Poll::Ready(Some(Err(_))) => 60,
                    Poll::Pending => 70,
                };
            }
            (rx, rx_cx, value)
        });
        let tx_value = tx.join().unwrap();
        let (mut rx, mut rx_cx, mut rx_value) = rx.join().unwrap();
        if !rx.is_terminated() {
            rx_value += match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                Poll::Ready(None) => 1,
                Poll::Ready(Some(Ok(value))) if value.get() == 13 => 3,
                Poll::Ready(Some(Ok(value))) if value.get() == 24 => 4,
                Poll::Ready(Some(Ok(_))) => 5,
                Poll::Ready(Some(Err(_))) => 6,
                Poll::Pending => 7,
            };
        }
        let key = tx_value + rx_value;
        statemap_put(rx_states, rx_counter, key);
    });
    statemap_check_exhaustive(rx_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_send_send_send_err_next_persistent() {
    let rx_states = statemap![
        12360 => [0, 101, 102, 10100],
        12736 => [102],
        14600 => [0, 101, 10100],
        14760 => [101],
        17236 => [103],
        17460 => [103],
        17746 => [103],
    ];
    let data_states = statemap![
        12360 => [10],
        12736 => [10],
        14600 => [10],
        14760 => [10],
        17236 => [10],
        17460 => [10],
        17746 => [10],
    ];
    loom::model(move || {
        async_context!(rx_counter, rx_waker, rx_cx);
        let (mut tx, mut rx) = channel::<CheckDrop>();
        check_drop!(data_counter, data, 314);
        let tx = loom::thread::spawn(move || match tx.send(11) {
            Ok(()) => match tx.send(13) {
                Ok(()) => match tx.send_err(data) {
                    Ok(()) => 10000,
                    Err(_) => 20000,
                },
                Err(_) => 30000,
            },
            Err(_) => 40000,
        });
        let rx = loom::thread::spawn(move || {
            let mut value = match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                Poll::Ready(None) => 1000,
                Poll::Ready(Some(Ok(value))) if value.get() == 11 => 2000,
                Poll::Ready(Some(Ok(value))) if value.get() == 13 => 3000,
                Poll::Ready(Some(Ok(value))) if value.get() == 24 => 4000,
                Poll::Ready(Some(Ok(_))) => 5000,
                Poll::Ready(Some(Err(_))) => 6000,
                Poll::Pending => 7000,
            };
            if !rx.is_terminated() {
                value += match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                    Poll::Ready(None) => 100,
                    Poll::Ready(Some(Ok(value))) if value.get() == 11 => 200,
                    Poll::Ready(Some(Ok(value))) if value.get() == 13 => 300,
                    Poll::Ready(Some(Ok(value))) if value.get() == 24 => 400,
                    Poll::Ready(Some(Ok(_))) => 500,
                    Poll::Ready(Some(Err(value))) => {
                        assert_eq!(value.get(10), 314);
                        600
                    }
                    Poll::Pending => 700,
                };
            }
            (rx, rx_cx, value)
        });
        let tx_value = tx.join().unwrap();
        let (mut rx, mut rx_cx, mut rx_value) = rx.join().unwrap();
        if !rx.is_terminated() {
            rx_value += match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                Poll::Ready(None) => 10,
                Poll::Ready(Some(Ok(value))) if value.get() == 13 => 30,
                Poll::Ready(Some(Ok(value))) if value.get() == 24 => 40,
                Poll::Ready(Some(Ok(_))) => 50,
                Poll::Ready(Some(Err(value))) => {
                    assert_eq!(value.get(10), 314);
                    60
                }
                Poll::Pending => 70,
            };
        }
        if !rx.is_terminated() {
            rx_value += match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                Poll::Ready(None) => 1,
                Poll::Ready(Some(Ok(_))) => 5,
                Poll::Ready(Some(Err(value))) => {
                    assert_eq!(value.get(10), 314);
                    6
                }
                Poll::Pending => 7,
            };
        }
        let key = tx_value + rx_value;
        statemap_put(rx_states, rx_counter, key);
        statemap_put(data_states, data_counter, key);
    });
    statemap_check_exhaustive(rx_states);
    statemap_check_exhaustive(data_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_send_err_close_next() {
    let rx_states = statemap![
        12 => [0],
        20 => [0],
    ];
    let data_states = statemap![
        12 => [10],
        20 => [10],
    ];
    loom::model(move || {
        async_context!(rx_counter, rx_waker, rx_cx);
        let (tx, mut rx) = channel::<CheckDrop>();
        check_drop!(data_counter, data, 314);
        let tx = loom::thread::spawn(move || match tx.send_err(data) {
            Ok(()) => 10,
            Err(value) => {
                assert_eq!(value.get(10), 314);
                20
            }
        });
        let rx = loom::thread::spawn(move || {
            rx.close();
            match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                Poll::Ready(None) => {
                    assert!(rx.is_terminated());
                    0
                }
                Poll::Ready(Some(Ok(_))) => 1,
                Poll::Ready(Some(Err(value))) => {
                    assert!(rx.is_terminated());
                    assert_eq!(value.get(10), 314);
                    2
                }
                Poll::Pending => 3,
            }
        });
        let key = tx.join().unwrap() + rx.join().unwrap();
        statemap_put(rx_states, rx_counter, key);
        statemap_put(data_states, data_counter, key);
    });
    statemap_check_exhaustive(rx_states);
    statemap_check_exhaustive(data_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_send_err_try_next() {
    let data_states = statemap![
        12 => [10],
        13 => [1],
        23 => [10],
    ];
    loom::model(move || {
        let (tx, mut rx) = channel::<CheckDrop>();
        check_drop!(data_counter, data, 314);
        let tx = loom::thread::spawn(move || match tx.send_err(data) {
            Ok(()) => 10,
            Err(value) => {
                assert_eq!(value.get(10), 314);
                20
            }
        });
        let rx = loom::thread::spawn(move || match rx.try_next() {
            Err(TryNextError::Canceled) => 0,
            Ok(Ok(_)) => 1,
            Ok(Err(value)) => {
                assert_eq!(value.get(10), 314);
                2
            }
            Err(TryNextError::Empty) => 3,
        });
        statemap_put(data_states, data_counter, tx.join().unwrap() + rx.join().unwrap());
    });
    statemap_check_exhaustive(data_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_send_err_try_next_persistent() {
    let data_states = statemap![
        12 => [10],
        15 => [10],
    ];
    loom::model(move || {
        let (tx, mut rx) = channel::<CheckDrop>();
        check_drop!(data_counter, data, 314);
        let tx = loom::thread::spawn(move || match tx.send_err(data) {
            Ok(()) => 10,
            Err(_) => 20,
        });
        let rx = loom::thread::spawn(move || {
            let value = match rx.try_next() {
                Err(TryNextError::Canceled) => 0,
                Ok(Ok(_)) => 1,
                Ok(Err(value)) => {
                    assert_eq!(value.get(10), 314);
                    2
                }
                Err(TryNextError::Empty) => 3,
            };
            (rx, value)
        });
        let tx_value = tx.join().unwrap();
        let (mut rx, mut rx_value) = rx.join().unwrap();
        if rx_value == 3 {
            rx_value = match rx.try_next() {
                Err(TryNextError::Canceled) => 3,
                Ok(Ok(_)) => 4,
                Ok(Err(value)) => {
                    assert_eq!(value.get(10), 314);
                    5
                }
                Err(TryNextError::Empty) => 6,
            };
        }
        statemap_put(data_states, data_counter, tx_value + rx_value);
    });
    statemap_check_exhaustive(data_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_send_err_close_try_next() {
    let data_states = statemap![
        12 => [10],
        20 => [10],
    ];
    loom::model(move || {
        let (tx, mut rx) = channel::<CheckDrop>();
        check_drop!(data_counter, data, 314);
        let tx = loom::thread::spawn(move || match tx.send_err(data) {
            Ok(()) => 10,
            Err(value) => {
                assert_eq!(value.get(10), 314);
                20
            }
        });
        let rx = loom::thread::spawn(move || {
            rx.close();
            match rx.try_next() {
                Err(TryNextError::Canceled) => 0,
                Ok(Ok(_)) => 1,
                Ok(Err(value)) => {
                    assert_eq!(value.get(10), 314);
                    2
                }
                Err(TryNextError::Empty) => 3,
            }
        });
        statemap_put(data_states, data_counter, tx.join().unwrap() + rx.join().unwrap());
    });
    statemap_check_exhaustive(data_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_next_cancellation() {
    let tx_states = statemap![
        13 => [101, 10100],
        10 => [10100],
        23 => [10100],
    ];
    let rx_states = statemap![
        13 => [101, 10100],
        10 => [0, 10100],
        23 => [10100],
    ];
    loom::model(move || {
        async_context!(tx_counter, tx_waker, tx_cx);
        async_context!(rx_counter, rx_waker, rx_cx);
        let (mut tx, mut rx) = channel::<CheckDrop>();
        let tx =
            loom::thread::spawn(move || match Pin::new(&mut tx.cancellation()).poll(&mut tx_cx) {
                Poll::Pending => 10,
                Poll::Ready(()) => 20,
            });
        let rx = loom::thread::spawn(move || match Pin::new(&mut rx).poll_next(&mut rx_cx) {
            Poll::Ready(None) => {
                assert!(rx.is_terminated());
                0
            }
            Poll::Ready(Some(Err(_))) => 1,
            Poll::Ready(Some(Ok(_))) => {
                assert!(rx.is_terminated());
                2
            }
            Poll::Pending => 3,
        });
        let key = tx.join().unwrap() + rx.join().unwrap();
        statemap_put(tx_states, tx_counter, key);
        statemap_put(rx_states, rx_counter, key);
    });
    statemap_check_exhaustive(tx_states);
    statemap_check_exhaustive(rx_states);
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_send_long_sequence_next_persistent() {
    loom::model(move || {
        async_context!(rx_counter, rx_waker, rx_cx);
        let (mut tx, mut rx) = channel::<CheckDrop>();
        let tx = loom::thread::spawn(move || {
            for _ in 0..6 {
                tx.send(1).unwrap();
            }
        });
        let rx = loom::thread::spawn(move || {
            let mut sum = 0;
            sum += match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                Poll::Ready(value) => value.unwrap().unwrap().get(),
                Poll::Pending => 0,
            };
            (rx, rx_cx, sum)
        });
        tx.join().unwrap();
        let (mut rx, mut rx_cx, mut rx_sum) = rx.join().unwrap();
        while !rx.is_terminated() {
            rx_sum += match Pin::new(&mut rx).poll_next(&mut rx_cx) {
                Poll::Ready(value) => value.unwrap().unwrap().get(),
                Poll::Pending => 0,
            };
        }
        assert_eq!(rx_sum, 6);
    });
}

#[cfg_attr(not(loom), ignore)]
#[test]
fn loom_send_long_sequence_try_next_persistent() {
    loom::model(move || {
        let (mut tx, mut rx) = channel::<CheckDrop>();
        let tx = loom::thread::spawn(move || {
            for _ in 0..6 {
                tx.send(1).unwrap();
            }
        });
        let rx = loom::thread::spawn(move || {
            let mut sum = 0;
            sum += match rx.try_next() {
                Ok(value) => value.unwrap().get(),
                Err(TryNextError::Empty) => 0,
                Err(TryNextError::Canceled) => panic!(),
            };
            (rx, sum)
        });
        tx.join().unwrap();
        let (mut rx, mut rx_sum) = rx.join().unwrap();
        while !rx.is_terminated() {
            rx_sum += match rx.try_next() {
                Ok(value) => value.unwrap().get(),
                Err(TryNextError::Empty) => 0,
                Err(TryNextError::Canceled) => panic!(),
            };
        }
        assert_eq!(rx_sum, 6);
    });
}
