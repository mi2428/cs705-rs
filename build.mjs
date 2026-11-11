import { copyFileSync, mkdirSync, writeFileSync } from 'node:fs'
import pug from 'pug'

const html = pug.renderFile('index.pug')
mkdirSync('docs', { recursive: true })
writeFileSync('docs/index.html', html)
copyFileSync('assets/screenshot.png', 'docs/screenshot.png')
