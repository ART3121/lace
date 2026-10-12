import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import globals from 'globals';
import tseslint from 'typescript-eslint';

const childProcess = {
  message: 'Só src/lace/cliClient.ts roda o lace: vistas e comandos falam com o LaceClient.',
};
const noChildProcess = [
  { name: 'child_process', ...childProcess },
  { name: 'node:child_process', ...childProcess },
];
const noVscode = [
  {
    name: 'vscode',
    message: 'src/lace/ não conhece o VS Code: é o que os testes unitários rodam fora dele.',
  },
];

export default defineConfig(
  { ignores: ['dist/', 'out/', '.vscode-test/', 'media/rtl-viewer/', 'src/lace/lace-types.ts'] },
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
  // processos (um `lace lsp` futuro troca só ele), e o cliente do Lace não
  // depende do VS Code.
  {
    files: ['src/**/*.ts'],
    ignores: ['src/lace/**'],
    rules: { 'no-restricted-imports': ['error', { paths: noChildProcess }] },
  },
  {
    files: ['src/lace/**/*.ts'],
    ignores: ['src/lace/cliClient.ts'],
    rules: { 'no-restricted-imports': ['error', { paths: [...noChildProcess, ...noVscode] }] },
  },
  {
    files: ['src/lace/cliClient.ts'],
    rules: { 'no-restricted-imports': ['error', { paths: noVscode }] },
  },
);
