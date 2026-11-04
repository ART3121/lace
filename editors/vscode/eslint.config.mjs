import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import globals from 'globals';
import tseslint from 'typescript-eslint';

const childProcess = {
  message: 'Só src/aurora/cliClient.ts roda o aurora: vistas e comandos falam com o AuroraClient.',
};
const noChildProcess = [
  { name: 'child_process', ...childProcess },
  { name: 'node:child_process', ...childProcess },
];
const noVscode = [
  {
    name: 'vscode',
    message: 'src/aurora/ não conhece o VS Code: é o que os testes unitários rodam fora dele.',
  },
];

export default defineConfig(
  { ignores: ['dist/', 'out/', '.vscode-test/', 'media/rtl-viewer/', 'src/aurora/aurora-types.ts'] },
  js.configs.recommended,
  tseslint.configs.recommended,
  {
    // `const { a, ...resto } = x` tira `a` de propósito.
    rules: { '@typescript-eslint/no-unused-vars': ['error', { ignoreRestSiblings: true }] },
  },
  {
    files: ['**/*.mjs'],
    languageOptions: { globals: globals.node },
  },
  // As duas decisões de arquitetura que o lint cobra: um único módulo cria
  // processos (um `aurora lsp` futuro troca só ele), e o cliente da AURORA não
  // depende do VS Code.
  {
    files: ['src/**/*.ts'],
    ignores: ['src/aurora/**'],
    rules: { 'no-restricted-imports': ['error', { paths: noChildProcess }] },
  },
  {
    files: ['src/aurora/**/*.ts'],
    ignores: ['src/aurora/cliClient.ts'],
    rules: { 'no-restricted-imports': ['error', { paths: [...noChildProcess, ...noVscode] }] },
  },
  {
    files: ['src/aurora/cliClient.ts'],
    rules: { 'no-restricted-imports': ['error', { paths: noVscode }] },
  },
);
