// const https = require('https');
const http2 = require('http2');

const fs = require('fs');
const tls = require('tls');

tls.DEFAULT_MAX_VERSION = 'TLSv1.3';

const options = {
  key: fs.readFileSync('/home/dennis/workspace/dacha/testdata/certificates/server.key'),
  cert: fs.readFileSync('/home/dennis/workspace/dacha/testdata/certificates/server.crt')
};

const server = http2.createSecureServer(options);
server.on('error', (err) => console.error(err));

server.on('stream', (stream, headers) => {
  // stream is a Duplex
  stream.respond({
    'content-type': 'text/html; charset=utf-8',
    ':status': 200
  });
  stream.end('<h1>Hello World</h1>');
});

server.listen(8001);


/*
https.createServer(options, (req, res) => {
  res.writeHead(200);
  res.end('hello world\n');
}).listen(8001);
*/

/*
const http = require('http');

const hostname = '127.0.0.1';
const port = 3000;

const server = http.createServer((req, res) => {
  console.log('URL: ' + req.url);
  res.statusCode = 200;
  res.setHeader('Content-Type', 'text/plain');
  res.end('Hello World');
});

server.listen(port, hostname, () => {
  console.log(`Server running at http://${hostname}:${port}/`);
});
*/