classify() {
    local n="$1"
    if (( n < 0 )); then
        echo "negative"
        return 0
    fi
    if (( n == 0 )); then
        echo "zero"
        return 0
    fi
    if (( n < 10 )); then
        echo "small"
        return 0
    fi
    echo "large"
}

main() {
    echo "classify -5 $(classify -5)"
    echo "classify 0 $(classify 0)"
    echo "classify 7 $(classify 7)"
    echo "classify 40 $(classify 40)"
    local i=0
    while (( i < 6 )); do
        i=$(( i + 1 ))
        if (( i % 2 == 0 )); then
            continue
        fi
        if (( i == 5 )); then
            break
        fi
        echo "odd ${i}"
    done
    for value in 3 1 2; do
        echo "item ${value}"
    done
    local outer=0
    while (( outer < 3 )); do
        local inner=0
        while (( inner < 3 )); do
            if (( inner == 2 )); then
                break
            fi
            echo "pair ${outer} ${inner}"
            inner=$(( inner + 1 ))
        done
        outer=$(( outer + 1 ))
    done
    echo "done"
}

main
