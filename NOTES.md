Program will fail with "operation not permitted" when not run as root. To be able to run as non-root:
- `sudo sysctl -w net.ipv4.ping_group_range='0 2147483647'`
- `sudo setcap CAP_NET_RAW+eip /path/to/program`
