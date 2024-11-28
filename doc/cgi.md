# CGI: Common Gateway Interface

Nanobot can be run as a
[CGI script](https://en.wikipedia.org/wiki/Common_Gateway_Interface).
This is an old-fashioned but simple and flexible
way to deploy a web application.
It's particularly easy to call use Nanobot CGI from a "wrapper" server,
written in another language, such as Python.

To run Nanobot as a CGI script
you just execute the `nanobot` binary,
with some specific environment variables
and optional `STDIN` input,
and it will return an HTTP response on `STDOUT`.

The CGI environment variables are:

- `GATEWAY_INTERFACE` set to `CGI/1.1` -- required
- `REQUEST_METHOD` for the HTTP method, defaults to `GET`
- `PATH_INFO` with the path part of the URL, defaults to `/table`
- `QUERY_STRING` with the query string part of the URL, which is optional

Nanobot also checks these environment variables:

- `NANOBOT_READONLY`: when `TRUE` no editing is supporting; defaults to `FALSE`

Nanobot will return an
[HTTP response](https://en.wikipedia.org/wiki/HTTP#HTTP/1.1_response_messages),
with a status line,
zero or more lines of HTTP headers,
and blank line,
and an optional message body
which will usually contain the HTML or JSON response content.

Nanobot's CGI mode works by
starting the same HTTP server used for `nanobot serve` on a random port,
executing the request,
and printing the response to `STDOUT`.
This is much less efficient than a long-running server,
but it's very simple
and works well enough for low volumes of traffic.

You can test Nanobot CGI from the command-line like so:

```console tesh-session="cgi"
$ nanobot init
Initialized a Nanobot project
$ GATEWAY_INTERFACE=CGI/1.1 PATH_INFO=/table.txt nanobot
status: 200 OK
content-type: text/plain
content-length: 291
date: ...

table     path                     type      description
table     src/schema/table.tsv     table     All of the tables in this project.
column    src/schema/column.tsv    column    Columns for all of the tables.
datatype  src/schema/datatype.tsv  datatype  Datatypes for all of the columns
```

We can POST a new row using CGI and form contents as `STDIN`:

```console tesh-session="cgi"
$ export GATEWAY_INTERFACE=CGI/1.1
$ REQUEST_METHOD=POST PATH_INFO=/table nanobot <<< 'action=submit&table=foo'
...
$ PATH_INFO=/table.txt nanobot
status: 200 OK
content-type: text/plain
content-length: 337
date: ...

table     path                     type      description
table     src/schema/table.tsv     table     All of the tables in this project.
column    src/schema/column.tsv    column    Columns for all of the tables.
datatype  src/schema/datatype.tsv  datatype  Datatypes for all of the columns
foo
```

When `NANOBOT_READONLY` is `TRUE`,
POSTing will not work,
and the WebUI will not include buttons for editing actions.

```console tesh-session="cgi"
$ export NANOBOT_READONLY=TRUE
$ REQUEST_METHOD=POST PATH_INFO=/table nanobot <<< 'action=submit&table=bar'
status: 403 Forbidden
content-type: text/html; charset=utf-8
content-length: 13
date: ...

403 Forbidden
```

## Python

You can run Nanobot CGI from any language that can "shell out" to another process.
In Python, you can use the `subprocess` module, like so:

```python
import subprocess

result = subprocess.run(
    ['bin/nanobot')],
    env={
        'GATEWAY_INTERFACE': 'CGI/1.1',
        'REQUEST_METHOD': 'GET',
        'PATH_INFO': path,
        'QUERY_STRING': request.query_string,
    },
    input=request.body.getvalue().decode('utf-8'),
    text=True,
    capture_output=True
)
print(result.stdout)
```

If you're already running a Python server,
such as Flask or Bottle,
that provides a `request` and `response`,
then you can "wrap" Nanobot inside the Python server
with code similar to this:

```python
import subprocess
from bottle import request, response

def run_cgi(path):
    # Run nanobot as a CGI script.
    result = subprocess.run(
        ['bin/nanobot')],
        env={
            'GATEWAY_INTERFACE': 'CGI/1.1',
            'REQUEST_METHOD': 'GET',
            'PATH_INFO': path,
            'QUERY_STRING': request.query_string,
        },
        input=request.body.getvalue().decode('utf-8'),
        text=True,
        capture_output=True
    )
    # Parse the HTTP response: status, headers, blank line, body.
    reading_headers = True
    body = []
    for line in result.stdout.splitlines():
        # Watch for the blank line that separates HTTP headers from the body.
        if reading_headers and line.strip() == '':
            reading_headers = False
            continue
        # Add each HTTP header to the `response`.
        if reading_headers:
            name, value = line.split(': ', 1)
            if name == 'status':
                response.status = value
            else:
                response.set_header(name, value)
        # Add all remanining lines to the `response` body.
        else:
            body.append(line)
    # The `response` is set, so just return the body.
    return '\n'.join(body)
```
