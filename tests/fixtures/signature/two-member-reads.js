function adjust(a, b) {
    return a + b;
}

const h = {
    get value() {
        console.log("read");
        return 1;
    }
};
adjust(h.value, h.value);
