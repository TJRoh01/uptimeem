use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::RwLock;

use crate::color::Color;

#[derive(Clone)]
pub struct SharedState {
    inner: Arc<(AtomicU64, RwLock<HashMap<IpAddr, RwLock<Metric>>>)>
}

#[derive(Debug)]
struct Metric {
    availability_by_avg: (&'static str, Color),
    availability_by_loss: (&'static str, Color),
    t_pings: u16, // total pings
    s_pings: u16, // successful pings
    f_pings: u16  // failed pings
}

impl SharedState {
    pub fn new() -> Self {
        Self { inner: Arc::new((AtomicU64::new(0), RwLock::new(HashMap::new()))) }
    }

    pub fn get_num_tracked(&self) -> u64 {
        self.inner.0.load(Ordering::Relaxed)
    }

    pub async fn insert(&self, ip: IpAddr) {
        self.inner.0.fetch_add(1, Ordering::Relaxed);

        self.inner.1.write().await.insert(ip, RwLock::new(Metric {
            availability_by_avg: ("??%", Color::LightGrey),
            availability_by_loss: (">99.99%", Color::BrightGreen),
            t_pings: 0,
            s_pings: 0,
            f_pings: 0
        }));
    }

    pub async fn get_availability_by_avg(&self, ip: &IpAddr) -> Option<(&'static str, &'static str)> {
        match self.inner.1.read().await.get(ip) {
            Some(x) => {
                let lock = x.read().await;
                Some((lock.availability_by_avg.0, lock.availability_by_avg.1.as_str()))
            },
            _ => None
        }
    }

    pub async fn get_availability_by_loss(&self, ip: &IpAddr) -> Option<(&'static str, &'static str)> {
        match self.inner.1.read().await.get(ip) {
            Some(x) => {
                let lock = x.read().await;
                Some((lock.availability_by_loss.0, lock.availability_by_loss.1.as_str()))
            },
            _ => None
        }
    }

    fn decode_availability(x: f64) -> (&'static str, Color) {
        match x {
            x if x >= 0.99995 => (">99.99%", Color::BrightGreen),
            x if x >= 0.9999 => ("99.99%", Color::BrightGreen),
            x if x >= 0.9995 => ("99.95%", Color::Green),
            x if x >= 0.999 => ("99.9%", Color::Green),
            x if x >= 0.998 => ("99.8%", Color::YellowGreen),
            x if x >= 0.995 => ("99.5%", Color::YellowGreen),
            x if x >= 0.99 => ("99%", Color::Yellow),
            x if x >= 0.98 => ("98%", Color::Yellow),
            x if x >= 0.97 => ("97%", Color::Orange),
            x if x >= 0.95 => ("95%", Color::Orange),
            x if x >= 0.90 => ("90%", Color::Red),
            _ => ("<90%", Color::Red)
        }
    }

    pub async fn succ_ping(&self, ip: &IpAddr) -> bool {
        let shared_state_lock = self.inner.1.read().await;
        let mut my_state_lock = match shared_state_lock.get(ip) {
            Some(x) => x.write().await,
            _ => return false
        };

        if my_state_lock.t_pings == u16::MAX {
            (*my_state_lock).t_pings = 0;
            (*my_state_lock).s_pings = 0;
            (*my_state_lock).f_pings = 0;
            (*my_state_lock).availability_by_avg = ("??%", Color::LightGrey);
            (*my_state_lock).availability_by_loss = (">99.99%", Color::BrightGreen);
        } else {
            my_state_lock.t_pings += 1;
            my_state_lock.s_pings += 1;
            (*my_state_lock).availability_by_avg = Self::decode_availability(my_state_lock.s_pings as f64 / my_state_lock.t_pings as f64);
        }

        true
    }

    pub async fn fail_ping(&self, ip: &IpAddr) -> bool {
        let shared_state_lock = self.inner.1.read().await;
        let mut my_state_lock = match shared_state_lock.get(ip) {
            Some(x) => x.write().await,
            _ => return false
        };

        if my_state_lock.t_pings == u16::MAX {
            (*my_state_lock).t_pings = 0;
            (*my_state_lock).s_pings = 0;
            (*my_state_lock).f_pings = 0;
            (*my_state_lock).availability_by_avg = ("??%", Color::LightGrey);
            (*my_state_lock).availability_by_loss = (">99.99%", Color::BrightGreen);
        } else {
            my_state_lock.t_pings += 1;
            my_state_lock.f_pings += 1;
            (*my_state_lock).availability_by_avg = Self::decode_availability(my_state_lock.s_pings as f64 / my_state_lock.t_pings as f64);
            (*my_state_lock).availability_by_loss = Self::decode_availability((u16::MAX - my_state_lock.f_pings) as f64 / u16::MAX as f64);
        }

        true
    }
}