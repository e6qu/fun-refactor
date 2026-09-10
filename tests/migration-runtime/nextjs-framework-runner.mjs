const url = process.argv[2]
const payload = process.argv[3]

let lastError
for (let attempt = 0; attempt < 80; attempt += 1) {
  try {
    const response = await fetch(url, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: payload,
    })
    const body = await response.json()
    console.log(JSON.stringify({ status: response.status, body }))
    process.exit(0)
  } catch (error) {
    lastError = error
    await new Promise((resolve) => setTimeout(resolve, 250))
  }
}

throw lastError
