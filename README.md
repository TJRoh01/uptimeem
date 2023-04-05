# uptime 'em
`uptime 'em` is a simple host uptime tracker that provides JSON endpoints for [shields.io](https://shields.io).  

Number of hosts currently tracked by the service **->** ![uptimeem num_tracked](https://img.shields.io/endpoint?url=https://api.uptimeem.com/num_tracked)  
Current version hosted at `api.uptimeem.com` **->** ![uptimeem version](https://img.shields.io/endpoint?url=https://api.uptimeem.com/version)

It is designed around efficiency and simplicity, which means it does not use a lot of resources and in turn that allows
me to make it available free of charge. There are no API-keys, accounts, etc. 

## Usage
How the service works:
- Sends an ICMP ping every **15 seconds** to hosts in its list
- Resets ping counter every `2^16 - 1` pings (per host)
- When the uptime of a host is requested for the first time it is added to its list of hosts, and the uptime will be
actively tracked from then on
- The shields.io CDN caches content for 5 minutes, so your badge can not update more than once every 5 minutes
- Buffers data for 4 minutes after a counter reset to prevent big uptime changes in `by_avg` mode after
a counter reset (read more under `MODE` parameter)

Output/Errors:
- Upon initial request there is not enough data, so a gray-colored badge showing `uptime: ??%` will be generated
- Once the badge is reloaded after 5 minutes (shields.io CDN cache period) the badge will be color-coded and show actual uptime as:
  - `>99.99%` - `bright-green`
  - `99.99%` - `bright-green`
  - `99.95%` - `green`
  - `99.9%` - `green`
  - `99.8%` - `yellow-green`
  - `99.5%` - `yellow-green`
  - `99%` - `yellow`
  - `98%` - `yellow`
  - `97%` - `orange`
  - `95%` - `orange`
  - `90%` - `red`
  - `<90%` - `red`
- If some parameter is invalid the badge will be red and say "invalid parameters"
- If the `HOST` parameter is a domain, and it could not be resolved using DNS, the badge will be red and say
"unresolvable hostname"
- Sometimes a badge can revert to `??%` despite already having been tracked for more than 4 minutes, this is the result
of a domain's DNS resolving to different IP-addresses

Format:
- Markdown: `![uptime by uptimeem](https://img.shields.io/endpoint?url=https://api.uptimeem.com/<HOST>/<MODE>)`
- HTML: `<img alt="uptime by uptimeem" src="https://img.shields.io/endpoint?url=https://api.uptimeem.com/<HOST>/<MODE>">`

Examples:
- `![uptime by uptimeem](https://img.shields.io/endpoint?url=https://api.uptimeem.com/1.1.1.1/by_avg)` -> ![uptime by uptimeem](https://img.shields.io/endpoint?url=https://api.uptimeem.com/1.1.1.1/by_avg)
- `![uptime by uptimeem](https://img.shields.io/endpoint?url=https://api.uptimeem.com/rust-lang.org/by_loss)` -> ![uptime by uptimeem](https://img.shields.io/endpoint?url=https://api.uptimeem.com/rust-lang.org/by_loss)

`HOST` parameter:
- Can be `IPv4`, `IPv6`, `domain.tld`, `(subsubdomain).subdomain.domain.tld`
- Do **NOT** include protocol (`http://`, `sftp://`, etc. are invalid)
- Do **NOT** include port (mydomain.com`:22`, 101.102.103.104`:8080`, etc. are invalid)

`MODE` parameter:  
I could not make up my mind which one of the following is better, so I decided to implement both and let you
choose instead. The main difference is that once you "loose" uptime in the `by_loss` mode there is no way to "get it back"
until the counter resets, meanwhile in the `by_avg` mode any longer downtime after a recent counter reset will have a
big effect on the reported uptime (since it is an average). I plan on adding a moving average in the future, once I know
exactly how much resources the current implementations require.
- `by_avg` - `number_successful_pings / number_total_pings`
- `by_loss` - `(2^16 - 1 - number_failed_pings) / (2^16 - 1)`

## Note
**PLEASE** do not request the service (api.uptimeem.com) directly, and always go through [shields.io](https://shields.io)!
This is to not put too much strain on the service itself.
