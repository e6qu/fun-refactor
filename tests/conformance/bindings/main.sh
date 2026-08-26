main() {
    echo "start"
    local n=42
    local total=$(( n + 10 ))
    echo "n ${n}"
    echo "sum ${total}"
    total=$(( total * 2 ))
    echo "twice ${total}"
    local q=$(( 10 / 3 ))
    local r=$(( 10 % 3 ))
    echo "q ${q} r ${r}"
    local label="item-7"
    echo "label ${label}"
    local i=0
    while (( i < 3 )); do
        echo "tick ${i}"
        i=$(( i + 1 ))
    done
    echo "done"
}

main
