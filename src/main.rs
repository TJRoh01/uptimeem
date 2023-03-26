use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use hyper::{Body, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};

use surge_ping::{Client, Config, ICMP, PingIdentifier, PingSequence};
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinSet;
use tokio::time::MissedTickBehavior;

const PING_INTERVAL: Duration = Duration::from_secs(5);
const PING_TIMEOUT: Duration = Duration::from_secs(3);

type SharedJoinSet = Arc<Mutex<JoinSet<()>>>;
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
            availability: "??%",
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
    let shared_join_set: SharedJoinSet = Arc::new(Mutex::new(JoinSet::new()));
    let shared_state: SharedState = Arc::new(RwLock::new(HashMap::new()));
    let client_v4 = Client::new(&Config::builder().kind(ICMP::V4).build()).unwrap();
    let client_v6 = Client::new(&Config::builder().kind(ICMP::V6).build()).unwrap();

    /*
    if let Some(interface) = opt.iface {
        config_builder = config_builder.interface(&interface);
    }
    */

    let make_service = make_service_fn(move |_conn| {
        let shared_join_set = shared_join_set.clone();
        let shared_state = shared_state.clone();
        let client_v4 = client_v4.clone();
        let client_v6 = client_v6.clone();

        let service = service_fn(move |req|
            handle(shared_join_set.clone(), shared_state.clone(), client_v4.clone(), client_v6.clone(), req)
        );

        async move { Ok::<_, Infallible>(service) }
    });

    let addr = SocketAddr::from(([127, 0, 0, 1], 9999));
    let server = Server::bind(&addr).serve(make_service);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

async fn handle(
    shared_join_set: SharedJoinSet,
    shared_state: SharedState,
    client_v4: Client,
    client_v6: Client,
    req: Request<Body>,
) -> Result<Response<Body>, Infallible> {
    let ip_str = match &req.uri().path()[1..] {
        x if x.len() > 0 && x.len() < 256 => x,
        _ => return Ok(Response::new(Body::from("{\
            \"schemaVersion\": 1,\
            \"label\": \"uptime\",\
            \"message\": \"invalid hostname\",\
            \"color\": \"critical\",\
            \"isError\": true\
            }"
        )))
    };

    let ip_addr = match tokio::net::lookup_host(ip_str)
        .await
        .map(|mut x| x.next().map(|x| x.ip()))
    {
        Ok(Some(x)) => x,
        _ => return Ok(Response::new(Body::from("{\
            \"schemaVersion\": 1,\
            \"label\": \"uptime\",\
            \"message\": \"unreachable hostname\",\
            \"color\": \"critical\",\
            \"isError\": true\
            }"
        )))
    };

    if shared_state.read().await.get(&ip_addr).is_none() {
        shared_state.write().await.insert(ip_addr.clone(), Metric::new());

        if ip_addr.is_ipv4() {
            shared_join_set.lock().await.spawn(ping_loop(shared_state.clone(), client_v4.clone(), ip_addr.clone()));
        } else if ip_addr.is_ipv6() {
            shared_join_set.lock().await.spawn(ping_loop(shared_state.clone(), client_v6.clone(), ip_addr.clone()));
        }

        return Ok(Response::new(Body::from("{\
            \"schemaVersion\": 1,\
            \"label\": \"uptime\",\
            \"message\": \"??%\",\
            }"
        )))
    } else {
        return Ok(Response::new(Body::from(format!("{{\
            \"schemaVersion\": 1,\
            \"label\": \"uptime\",\
            \"message\": \"{}\",\
            }}", shared_state.read().await.get(&ip_addr).unwrap().get_availability_str().await
        ))))
    }
}

// continuously ping given address
async fn ping_loop(shared_state: SharedState, client: Client, ip: IpAddr) {
    let mut interval = tokio::time::interval(PING_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let ping_identifier = PingIdentifier(rand::random());
    let mut n_sequence = 0;

    let mut pinger = client.pinger(ip, ping_identifier).await;
    pinger.timeout(PING_TIMEOUT);

    loop {
        interval.tick().await;

        let state_lock = shared_state.read().await;

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