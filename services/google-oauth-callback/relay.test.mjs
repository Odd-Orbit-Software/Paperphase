import {test} from 'node:test';
import assert from 'node:assert/strict';
import {handle, localReturn} from './relay.mjs';
const state = (return_to = 'http://192.168.1.53:8080/api/google/callback') => Buffer.from(JSON.stringify({nonce:'a'.repeat(43), return_to})).toString('base64url');
const request = params => new Request('https://auth.paperphase.oddorbit.software/oauth/google/callback?' + new URLSearchParams(params));

test('returns the browser to the Pi without an upstream request or token exchange', () => {
  const nonce=state();
  const response=handle(request({state:nonce,code:'fixture-code'}));
  assert.equal(response.status,303);
  const url=new URL(response.headers.get('location'));
  assert.equal(url.origin,'http://192.168.1.53:8080');
  assert.equal(url.pathname,'/api/google/callback');
  assert.equal(url.searchParams.get('code'),'fixture-code');
  assert.equal(url.searchParams.get('state'),nonce);
  assert.equal(response.headers.get('cache-control'),'no-store');
  assert.equal(response.headers.get('referrer-policy'),'no-referrer');
});
test('returns an explicit denial to the Pi for its pending-session check', () => {
  const response=handle(request({state:state(),error:'access_denied'}));
  assert.equal(response.status,303);
  assert.equal(new URL(response.headers.get('location')).searchParams.get('error'),'access_denied');
});
test('cannot redirect to arbitrary public sites or another local endpoint', () => {
  for (const value of ['https://attacker.example/api/google/callback','http://8.8.8.8/api/google/callback','http://192.168.1.1/admin','http://user:secret@192.168.1.53/api/google/callback','javascript:alert(1)','http://192.168.1.53/api/google/callback?next=https://attacker.example']) {
    assert.throws(()=>localReturn(value));
    assert.equal(handle(request({state:state(value),code:'fixture-code'})).status,400);
  }
  for (const host of ['10.0.0.2','172.16.0.2','192.168.1.53','127.0.0.1','paperphase','paperphase.local','[::1]','[fd00::1]']) assert.doesNotThrow(()=>localReturn(`http://${host}:8080/api/google/callback`));
});
test('malformed, duplicated and ambiguous responses are rejected without reflection', async () => {
  const urls=[request({state:'bad',code:'private-code'}),request({state:state()}),request({state:state(),code:'private-code',error:'access_denied'}),new Request(request({state:state(),code:'private-code'}).url+'&code=second')];
  for (const req of urls) {
    const response=handle(req);assert.equal(response.status,400);
    assert.ok(!(await response.text()).includes('private-code'));
  }
});
