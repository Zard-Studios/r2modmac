import { DependencyChoiceDialog } from './DependencyChoiceDialog';

interface UninstallModalProps {
    isOpen: boolean;
    modName: string;
    modIcon?: string;
    orphanDeps: { name: string; icon?: string }[];
    allDeps: { name: string; icon?: string }[];
    allDepsCount: number;
    onCancel: () => void;
    onModOnly: () => void;
    onWithOrphans: () => void;
    onWithAllDeps: () => void;
}

export function UninstallModal({
    isOpen,
    modName,
    modIcon,
    orphanDeps,
    allDeps,
    allDepsCount,
    onCancel,
    onModOnly,
    onWithOrphans,
    onWithAllDeps,
}: UninstallModalProps) {
    if (!isOpen) return null;

    const orphanCount = orphanDeps.length;
    const orphanNames = new Set(orphanDeps.map((dep) => dep.name.toLowerCase()));
    const isUnused = (name: string) => orphanNames.has(name.toLowerCase());

    const options = [
        {
            id: 'mod-only',
            label: 'Remove mod only',
            hint: 'Keep all dependencies.',
            affects: () => false,
        },
        ...(orphanCount > 0 ? [{
            id: 'orphans',
            label: 'Remove with unused dependencies',
            hint: 'Cleans up files nothing else needs.',
            count: orphanCount,
            affects: (row: { name: string }) => isUnused(row.name),
        }] : []),
        ...(allDepsCount > orphanCount ? [{
            id: 'all',
            label: 'Remove with all dependencies',
            hint: 'May break other mods.',
            count: allDepsCount,
            affects: () => true,
        }] : []),
    ];

    return (
        <DependencyChoiceDialog
            title={`Uninstall ${modName}`}
            subtitle="Choose what happens to its installed dependencies."
            mod={{ name: modName, icon: modIcon }}
            listLabel="Installed dependencies"
            rows={allDeps.map((dep) => ({ ...dep, unused: isUnused(dep.name) }))}
            options={options}
            defaultOptionId={orphanCount > 0 ? 'orphans' : 'mod-only'}
            confirmLabel="Uninstall"
            confirmTone="danger"
            onConfirm={(id) => {
                if (id === 'orphans') onWithOrphans();
                else if (id === 'all') onWithAllDeps();
                else onModOnly();
            }}
            onCancel={onCancel}
        />
    );
}
