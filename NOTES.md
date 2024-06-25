todo: ui loop and data loop should be isolated and use channels

find a timeout option for the UI polling for keystrokes or better yet,
find an interrupt


map:

replicas -+- pod -+- containers -+- logs
          |       +- logs
          |       +- tcpdump
          |
          +- ingress -+- certs

or... 

top level hidden nav is "Nodes" "ReplicaSets"

replicaset page and all sub pages have a 50/50 vertical section, lower section
is tabbed details of: facts( requests, limits, image, ports, healthurls), events
