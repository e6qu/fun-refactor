main() {
    local nums=()
    nums+=(3)
    nums+=(1)
    nums+=(2)
    echo "len ${#nums[@]}"
    echo "first ${nums[0]}"
    nums[1]=10
    local total=0
    for value in "${nums[@]}"; do
        total=$(( total + value ))
    done
    echo "sum ${total}"
    for value in "${nums[@]}"; do
        echo "item ${value}"
    done
    echo "done"
}

main
