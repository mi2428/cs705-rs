import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import pug from 'pug'

test('the published page matches the bilingual Pug source', () => {
  const html = readFileSync('docs/index.html', 'utf8')
  assert.equal(html, pug.renderFile('index.pug'))
  assert.match(html, /^<!DOCTYPE html>/)
  assert.match(html, /<html lang="ja">/)
  assert.match(html, /lang="en"/)
  assert.match(html, /name="viewport"/)
  assert.match(html, /<h1>cs705-rs<\/h1>/)
  assert.ok(html.includes('<p><a href="https://github.com/mi2428/cs705-rs">cs705-rs</a>は、'))
  assert.doesNotMatch(html, /<(?:header|section|h2|pre)\b/)
  assert.doesNotMatch(html, /examples\/|\b(?:CLI|TUI)\b/)
  const languages = [...html.matchAll(/<div lang="(ja|en)">/g)]
  assert.deepEqual(languages.map(match => match[1]), ['ja', 'en'])
  assert.doesNotMatch(html, /<(?:p|li) lang="en">/)
  assert.match(html, /<img src="screenshot\.png"[^>]*alt="[^"]+"/)
  assert.deepEqual(readFileSync('docs/screenshot.png'), readFileSync('assets/screenshot.png'))
  for (const url of [
    'https://github.com/mi2428/cs705-rs',
    'https://github.com/mi2428/cs705-rs/blob/main/NOTES.md',
    'https://www.icom.co.jp/lineup/products/IC-705/',
    'https://www.icom.co.jp/support/drivers/8597/',
    'https://www.icomjapan.com/lineup/products/IC-705/',
    'https://www.icomjapan.com/support/firmware_driver/4423/',
  ]) {
    assert.ok(html.includes(`href="${url}"`), url)
  }
  assert.match(html, /リバースエンジニアリング/)
  assert.match(html, /reverse engineering/)
})
