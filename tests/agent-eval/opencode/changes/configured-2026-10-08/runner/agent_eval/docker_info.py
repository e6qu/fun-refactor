"""Read resource-accounting fields from the required local Docker daemon."""
import http.client
import json
import socket

MAX_BYTES = 65536


def resource_info(path="/var/run/docker.sock"):
    """Avoid Docker CLI plugin discovery; the caller also bounds the process."""
    connection = http.client.HTTPConnection("localhost", timeout=10)
    channel = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        channel.settimeout(10)
        channel.connect(path)
        connection.sock = channel
        connection.request("GET", "/info", headers={"Connection": "close"})
        response = connection.getresponse()
        if response.status != 200:
            raise ValueError("Docker info HTTP status is not 200")
        data = response.read(MAX_BYTES + 1)
        if len(data) > MAX_BYTES:
            raise ValueError("Docker info exceeds the response limit")
        info = json.loads(data)
        if not isinstance(info, dict):
            raise ValueError("Docker info must be an object")
        driver, version, security = (info.get(key) for key in
                                     ("CgroupDriver", "CgroupVersion", "SecurityOptions"))
        if (not isinstance(driver, str) or not isinstance(version, (str, int))
                or isinstance(version, bool) or not isinstance(security, list)
                or not all(isinstance(option, str) for option in security)):
            raise ValueError("Docker info resource fields are invalid")
        return {"CgroupDriver": driver, "CgroupVersion": version, "SecurityOptions": security}
    finally:
        connection.close()
        channel.close()


if __name__ == "__main__":
    print(json.dumps(resource_info()))
