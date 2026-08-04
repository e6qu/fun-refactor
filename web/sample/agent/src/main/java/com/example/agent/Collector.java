package com.example.agent;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * The JVM half of the collector: it batches readings and hands them to a Sink.
 *
 * Java is here because it exercises the parts of this tool that only a language with
 * annotations and a container reaches. Nothing in this file calls `main`, and nothing
 * calls `handleBatch` — the JVM is pointed at one and Spring calls the other, which is
 * why `fr unused` needs an entry-point catalogue and not only a call graph.
 */
public class Collector {
    private final List<Reading> pending = new ArrayList<>();
    private final int batchSize;

    public Collector(int batchSize) {
        this.batchSize = batchSize;
    }

    /** The JVM is pointed at this. */
    public static void main(String[] args) {
        Collector collector = new Collector(64);
        collector.accept(new Reading("cpu", 0.9));
    }

    public void accept(Reading reading) {
        if (isAcceptable(reading)) {
            pending.add(reading);
            if (pending.size() >= batchSize) {
                flush();
            }
        }
    }

    /** A single expression, so `fr inline --call` has something to substitute. */
    private boolean isAcceptable(Reading reading) {
        return reading.value() >= 0;
    }

    public List<Reading> flush() {
        List<Reading> batch = List.copyOf(pending);
        pending.clear();
        return batch;
    }

    /**
     * Spring calls this; nothing here does. Without the `@RestController` rule in the
     * entry-point catalogue it looks exactly like dead code.
     */
    @RestController
    public Map<String, Integer> handleBatch(List<Reading> readings) {
        return Map.of("accepted", readings.size());
    }

    /** Genuinely unreferenced, and reported as such. */
    private int unusedTally() {
        return pending.size();
    }
}
