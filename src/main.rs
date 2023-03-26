use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use surge_ping::{Client, Config, ICMP, PingIdentifier, PingSequence};
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tokio::time::MissedTickBehavior;

const PING_INTERVAL: Duration = Duration::from_secs(5);
const PING_TIMEOUT: Duration = Duration::from_secs(3);

type SharedState = Arc<RwLock<HashMap<IpAddr, Metric>>>;

struct Metric(RwLock<MetricInner>);

#[derive(Debug)]
struct MetricInner {
    availability: &'static str,
    t_pings: u16, // total pings
    s_pings: u16  // successful pings
}

impl Metric {
    fn new() -> Self {
        Self(RwLock::new(MetricInner {
            availability: "EE%",
            t_pings: 0,
            s_pings: 0,
        }))
    }

    async fn inc_s_t_pings(&self) {
        self.check_reset().await;

        let mut inner = self.0.write().await;
        inner.t_pings += 1;
        inner.s_pings += 1;
        drop(inner);

        self.set_availability_str().await;
    }

    async fn inc_t_pings(&self) {
        self.check_reset().await;

        let mut inner = self.0.write().await;
        inner.t_pings += 1;
        drop(inner);

        self.set_availability_str().await;
    }

    async fn check_reset(&self) {
        if self.0.read().await.t_pings == u16::MAX {
            let mut inner = self.0.write().await;
            (*inner).t_pings = 0;
            (*inner).s_pings = 0;
        }
    }

    async fn set_availability_str(&self) {
        let mut inner = self.0.write().await;

        (*inner).availability = match (inner.s_pings as f64) / (inner.t_pings as f64) {
            x if x >= 0.99995 => ">99.99%",
            x if x >= 0.9999 => "99.99%",
            x if x >= 0.9995 => "99.95%",
            x if x >= 0.999 => "99.9%",
            x if x >= 0.998 => "99.8%",
            x if x >= 0.995 => "99.5%",
            x if x >= 0.99 => "99%",
            x if x >= 0.98 => "98%",
            x if x >= 0.97 => "97%",
            x if x >= 0.95 => "95%",
            x if x >= 0.90 => "90%",
            _ => "<90%"
        }
    }

    async fn get_availability_str(&self) -> &'static str {
        self.0.read().await.availability
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let state: SharedState = Arc::new(RwLock::new(HashMap::new()));

    let client_v4 = Client::new(&Config::builder().kind(ICMP::V4).build()).unwrap();
    let client_v6 = Client::new(&Config::builder().kind(ICMP::V6).build()).unwrap();

    /*
    if let Some(interface) = opt.iface {
        config_builder = config_builder.interface(&interface);
    }
    */

    let mut tasks = JoinSet::new();

    let ips = vec!["google.com", "1.1.1.1", "www.gitlab.com"];

    for ip in ips {
        let ip = tokio::net::lookup_host(format!("{}:0", ip))
            .await
            .expect("host lookup error")
            .next()
            .map(|val| val.ip())
            .unwrap();

        if state.read().await.get(&ip).is_none() {
            state.write().await.insert(ip.clone(), Metric::new());

            if ip.is_ipv4() {
                tasks.spawn(ping_loop(client_v4.clone(), ip, state.clone()));
            } else if ip.is_ipv6() {
                tasks.spawn(ping_loop(client_v6.clone(), ip, state.clone()));
            }
        }
    }

    while let Some(_) = tasks.join_next().await {}
}

// continuously ping given address
async fn ping_loop(client: Client, ip: IpAddr, state: SharedState) {
    let mut interval = tokio::time::interval(PING_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let ping_identifier = PingIdentifier(rand::random());
    let mut n_sequence = 0;

    let mut pinger = client.pinger(ip, ping_identifier).await;
    pinger.timeout(PING_TIMEOUT);

    loop {
        interval.tick().await;

        let state_lock = state.read().await;

        let state = match state_lock.get(&ip) {
            Some(x) => x,
            None => break
        };

        let ping_sequence = PingSequence(n_sequence);

        match pinger.ping(ping_sequence, &[]).await {
            Ok((packet, _rtt)) if packet.get_identifier() == ping_identifier && packet.get_sequence() == ping_sequence => {
                state.inc_s_t_pings().await;
            },
            _ => {
                state.inc_t_pings().await;
            }
        }

        drop(state_lock);

        if n_sequence == u16::MAX {
            n_sequence = 0;
        } else {
            n_sequence += 1;
        }
    }
}