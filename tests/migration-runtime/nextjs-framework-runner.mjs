const url = process.argv[2]
const payloads = process.argv.slice(3)
const results = []

for (const payload of payloads) {
  let lastError
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(url, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: payload,
      })
      const body = await response.json()
      results.push({ status: response.status, body })
      lastError = undefined
      break
    } catch (error) {
      lastError = error
      await new Promise((resolve) => setTimeout(resolve, 250))
    }
  }
  if (lastError !== undefined) throw lastError
}

console.log(JSON.stringify(results))
