// Caminhos como o netlist os traz: com / ou \, conforme o sistema que
// sintetizou.

export function baseName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
