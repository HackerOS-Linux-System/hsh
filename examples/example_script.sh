# example script — run via: source examples/example_script.sh
echo "starting up"
export GREETING=hello
echo "$GREETING, world"
echo "2 + 2 * 10 = $((2 + 2 * 10))"

function greet {
    echo "hi, $1!"
}
greet visitor

count=0
while [ $count -lt 3 ]; do
    count=$((count+1))
    if [ $count -eq 2 ]; then
        echo "count is two, skipping"
        continue
    fi
    echo "count=$count"
done

for lang in rust hsharp python; do
    echo "language: $lang"
done

echo "done"
