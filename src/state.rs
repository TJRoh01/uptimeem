use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use crate::uptime::Uptime;

#[derive(Clone)]
pub struct SharedState {
    inner: Arc<(AtomicU64, RwLock<HashMap<IpAddr, RwLock<Metric>>>)>
}

#[derive(Debug)]
struct Metric {
    uptime_by_avg: Uptime,
    uptime_by_loss: Uptime,
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
            uptime_by_avg: Uptime::UpUnknown,
            uptime_by_loss: Uptime::UpMax,
            t_pings: 0,
            s_pings: 0,
            f_pings: 0
        }));
    }

    pub async fn get_uptime_by_avg(&self, ip: &IpAddr) -> Option<(&'static str, &'static str)> {
        match self.inner.1.read().await.get(ip) {
            Some(x) => Some(x.read().await.uptime_by_avg.as_str()),
            _ => None
        }
    }

    pub async fn get_uptime_by_loss(&self, ip: &IpAddr) -> Option<(&'static str, &'static str)> {
        match self.inner.1.read().await.get(ip) {
            Some(x) => Some(x.read().await.uptime_by_loss.as_str()),
            _ => None
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
            (*my_state_lock).uptime_by_avg = Uptime::UpUnknown;
            (*my_state_lock).uptime_by_loss = Uptime::UpMax;
        } else {
            my_state_lock.t_pings += 1;
            my_state_lock.s_pings += 1;
            (*my_state_lock).uptime_by_avg = Uptime::from_f64(my_state_lock.s_pings as f64 / my_state_lock.t_pings as f64);
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
            (*my_state_lock).uptime_by_avg = Uptime::UpUnknown;
            (*my_state_lock).uptime_by_loss = Uptime::UpMax;
        } else {
            my_state_lock.t_pings += 1;
            my_state_lock.f_pings += 1;
            (*my_state_lock).uptime_by_avg = Uptime::from_f64(my_state_lock.s_pings as f64 / my_state_lock.t_pings as f64);
            (*my_state_lock).uptime_by_loss = Uptime::from_f64((u16::MAX - my_state_lock.f_pings) as f64 / u16::MAX as f64);
        }

        true
    }
}