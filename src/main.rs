use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use hyper::{Body, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use surge_ping::{Client, Config, ICMP, PingIdentifier, PingSequence};
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio::time::MissedTickBehavior;

use crate::state::SharedState;

mod state;

const PING_INTERVAL: Duration = Duration::from_secs(15);
const PING_TIMEOUT: Duration = Duration::from_secs(5);

type SharedJoinSet = Arc<Mutex<JoinSet<()>>>;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let shared_join_set: SharedJoinSet = Arc::new(Mutex::new(JoinSet::new()));
    let shared_state: SharedState = SharedState::new();
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

    match shared_state.get_availability_by_loss(&ip_addr).await {
        Some(x) => {
            return Ok(Response::new(Body::from(format!("{{\
                \"schemaVersion\": 1,\
                \"label\": \"uptime\",\
                \"message\": \"{}\"\
                }}", x
            ))))
        },
        None => {
            shared_state.insert(ip_addr.clone()).await;

            if ip_addr.is_ipv4() {
                shared_join_set.lock().await.spawn(ping_loop(shared_state.clone(), client_v4.clone(), ip_addr));
            } else if ip_addr.is_ipv6() {
                shared_join_set.lock().await.spawn(ping_loop(shared_state.clone(), client_v6.clone(), ip_addr));
            }

            return Ok(Response::new(Body::from("{\
                \"schemaVersion\": 1,\
                \"label\": \"uptime\",\
                \"message\": \"??%\"\
                }"
            )))
        }
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

        let ping_sequence = PingSequence(n_sequence);

        match pinger.ping(ping_sequence, &[]).await {
            Ok((packet, _rtt)) if packet.get_identifier() == ping_identifier && packet.get_sequence() == ping_sequence => {
                if !shared_state.succ_ping(&ip).await {
                    break
                }
            },
            _ => {
                if !shared_state.fail_ping(&ip).await {
                    break
                }
            }
        }

        if n_sequence == u16::MAX {
            n_sequence = 0;
        } else {
            n_sequence += 1;
        }
    }
}