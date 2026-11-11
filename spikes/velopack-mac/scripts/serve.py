import http.server, sys, os, functools

port = int(sys.argv[1]); root = sys.argv[2]
log = open(sys.argv[3], "a", buffering=1)

class H(http.server.SimpleHTTPRequestHandler):
    def log_message(self, fmt, *a):
        pass
    def do_GET(self):
        log.write("GET %s\n" % self.path)
        for k, v in self.headers.items():
            log.write("    %s: %s\n" % (k, v))
        super().do_GET()

http.server.ThreadingHTTPServer(("127.0.0.1", port), functools.partial(H, directory=root)).serve_forever()
