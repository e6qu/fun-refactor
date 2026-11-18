function adjust(a, b) {
    return a + b;
}

function run(n) {
    const h = {
        get value() {
            n++;
            return 1;
        }
    };
    return adjust(h.value, n);
}

run(2);
