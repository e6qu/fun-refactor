import requests
from fastapi import FastAPI
from fastapi.responses import JSONResponse

app = FastAPI()


@app.get("/records")
def list_records():
    return JSONResponse(content={"items": ["a", "b"]}, status_code=200)


@app.get("/feed")
def read_feed():
    return JSONResponse(content=requests.get("/records").json(), status_code=200)
