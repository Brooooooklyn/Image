const { execSync } = require('child_process')
const { writeFileSync } = require('fs')

const CL_REAL_PATH = execSync('which cl').toString().trim()

console.info(CL_REAL_PATH)

writeFileSync('bin/cl.bat', `${CL_REAL_PATH} /GL %*`)
