from fastapi import APIRouter
from pydantic import BaseModel


class EventEnvelope(BaseModel):
    event_id: str
    sentAt: int
    labels: list[str]


router = APIRouter()


@router.post("/events")
async def publish_event(event: EventEnvelope):
    return {
        "event_id": event.event_id,
        "sentAt": event.sentAt,
        "labels": event.labels,
    }
