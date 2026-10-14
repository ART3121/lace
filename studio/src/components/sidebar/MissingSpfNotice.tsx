// O aviso de quando o .spf do projeto aberto foi apagado (`spfMissing`): sem
// ele o Lace não faz operação de projeto. O botão abre o "Novo projeto" com a
// pasta e o nome deste projeto: criar ali grava um .spf novo, e os arquivos
// da pasta continuam (`Project::create`). Aparece no alto do Explorer e no
// lugar do Navegador de fluxo.

import { FolderPlus, TriangleAlert } from 'lucide-react';

import { useT } from '../../i18n';
import { openDialog } from '../../state/dialogs';
import { useProject } from '../../state/project';
import { baseName, dirName } from '../../util/paths';
import { Button } from '../common';

export function MissingSpfNotice() {
  const t = useT();
  const snapshot = useProject((s) => s.snapshot);
  if (!snapshot) return null;
  return (
    <div className="spf-missing" role="alert">
      <p className="spf-missing__title">
        <TriangleAlert size={15} />
        {t('explorer.spfMissing.title', { file: baseName(snapshot.spf) })}
      </p>
      <p>{t('explorer.spfMissing.body')}</p>
      <Button
        variant="primary"
        small
        icon={<FolderPlus size={14} />}
        onClick={() => openDialog({ kind: 'newProject', parent: dirName(snapshot.root), name: baseName(snapshot.root) })}
      >
        {t('explorer.spfMissing.create')}
      </Button>
      <p className="muted">{t('explorer.spfMissing.hint')}</p>
    </div>
  );
}
