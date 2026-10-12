import * as assert from 'node:assert/strict';

import { compareVersions, FEATURES, MIN_VERSION, parseVersionLine, supportedFeatures } from '../../src/lace/version';

describe('versão do Lace', () => {
  it('lê a linha do lace --version', () => {
    assert.equal(parseVersionLine('lace 0.6.0\n'), '0.6.0');
    assert.equal(parseVersionLine('lace 1.2.3-dev.4'), '1.2.3');
    assert.equal(parseVersionLine('solar 0.1.0'), undefined);
    assert.equal(parseVersionLine('lace'), undefined);
    assert.equal(parseVersionLine('Python 3.12.1'), undefined);
  });

  it('compara como números, não como texto', () => {
    assert.ok(compareVersions('0.10.0', '0.9.0') > 0);
    assert.ok(compareVersions('0.2.0', '0.2.0') === 0);
    assert.ok(compareVersions('0.1.9', '0.2.0') < 0);
    assert.ok(compareVersions('1.0.0', '0.99.99') > 0);
  });

  it('desliga o que a versão instalada não suporta', () => {
    const all = {
      newProject: true,
      sources: true,
      setTop: true,
      add: true,
      files: true,
      moveAfter: true,
      processors: true,
      wave: true,
      check: true,
      sim: true,
      simFast: true,
      synth: true,
    };
    const none = Object.fromEntries(Object.keys(all).map((k) => [k, false]));
    assert.deepEqual(supportedFeatures('0.1.0'), none);
    // Na 0.2.0 o check, o sim e o synth ainda escreviam outro JSON; o --fast é da 0.4.0.
    assert.deepEqual(supportedFeatures('0.2.0'), {
      ...all,
      moveAfter: false,
      check: false,
      sim: false,
      simFast: false,
      synth: false,
    });
    assert.deepEqual(supportedFeatures('0.3.0'), { ...all, moveAfter: false, simFast: false });
    assert.deepEqual(supportedFeatures('0.4.0'), { ...all, moveAfter: false });
    // O move --after é da versão depois da 0.6.0.
    assert.deepEqual(supportedFeatures('0.6.0'), { ...all, moveAfter: false });
    assert.deepEqual(supportedFeatures('0.7.0'), all);
  });

  it('MIN_VERSION é a do recurso mais antigo', () => {
    const oldest = Object.values(FEATURES).sort(compareVersions)[0];
    assert.equal(MIN_VERSION, oldest);
  });
});
