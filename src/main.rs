use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use surge_ping::{Client, Config, ICMP, PingIdentifier, PingSequence};
use tokio::sync::RwLock;
use tokio::task::JoinSet;
use tokio::time::MissedTickBehavior;

const PING_INTERVAL: Duration = Duration::from_secs(3);
const PING_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug)]
struct Metric {
    t_pings: u16, // total pings
    s_pings: u16  // successful pings
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let state: Arc<RwLock<HashMap<IpAddr, RwLock<Metric>>>> = Arc::new(RwLock::new(HashMap::new()));

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
            state.write().await.insert(ip.clone(), RwLock::new(Metric {
                t_pings: 0,
                s_pings: 0,
            }));

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
async fn ping_loop(client: Client, ip: IpAddr, state: Arc<RwLock<HashMap<IpAddr, RwLock<Metric>>>>) {
    let mut interval = tokio::time::interval(PING_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let ping_identifier = PingIdentifier(rand::random());
    let mut n_sequence = 0;

    let mut pinger = client.pinger(ip, ping_identifier).await;
    pinger.timeout(PING_TIMEOUT);

    loop {
        interval.tick().await;
        let ping_sequence = PingSequence(n_sequence);

        match pinger.ping(ping_sequence, &[]).await {
            Ok((packet, _rtt)) => {
                if packet.get_identifier() == ping_identifier && packet.get_sequence() == ping_sequence {
                    state.read().await.get(&ip).unwrap().write().await.s_pings += 1;
                }
            },
            _ => {}
        };

        if state.read().await.get(&ip).unwrap().read().await.t_pings == u16::MAX {
            state.read().await.get(&ip).unwrap().write().await.t_pings = 0;
            state.read().await.get(&ip).unwrap().write().await.s_pings = 0;
        } else {
            state.read().await.get(&ip).unwrap().write().await.t_pings += 1;
        }

        if n_sequence == u16::MAX {
            n_sequence = 0;
        } else {
            n_sequence += 1;
        }
    }
}