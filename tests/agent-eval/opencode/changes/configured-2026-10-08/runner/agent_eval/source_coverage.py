"""Join verified source pages without inventing unread bytes or crossing identities."""
from .study import require

POLICY = "contiguous-source-v1"


def contiguous(spans):
    """Callers verify each span against its snapshot before joining coverage."""
    require(isinstance(spans, list) and len(spans) <= 1024, "too many source spans")
    total = 0
    for span in spans:
        require(isinstance(span, dict) and all(isinstance(span.get(k), str) for k in ("path", "sha256", "text")), "invalid source span")
        require(type(span.get("start")) is int and type(span.get("end")) is int
                and 0 <= span["start"] <= span["end"], "invalid source extent")
        size = len(span["text"].encode())
        require(span["end"] - span["start"] == size, "source extent differs from text")
        total += size
        require(total <= 1024**2, "source coverage exceeds budget")
    merged = []
    for span in sorted(spans, key=lambda s: (s["path"], s["sha256"], s["start"], s["end"])):
        raw = span["text"].encode()
        if merged and (span["path"], span["sha256"]) == (merged[-1]["path"], merged[-1]["sha256"]) and span["start"] <= merged[-1]["end"]:
            previous = merged[-1]
            old = previous["text"].encode()
            relative = span["start"] - previous["start"]
            common = min(len(old) - relative, len(raw))
            require(old[relative:relative+common] == raw[:common], "overlapping source disagrees")
            previous.update(end=max(previous["end"], span["end"]), text=(old + raw[common:]).decode())
        else:
            merged.append({key: span[key] for key in ("path", "sha256", "start", "end", "text")})
    return merged
