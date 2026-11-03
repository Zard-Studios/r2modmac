import { DependencyChoiceDialog } from './DependencyChoiceDialog';

export interface DependencyToggleMod {
    name: string;
    version?: string | null;
    icon?: string;
}

interface DependencyToggleModalProps {
    isOpen: boolean;
    /** True when the mod is being turned on, false when it is being turned off. */
    enabling: boolean;
    mod: DependencyToggleMod;
    dependencies: DependencyToggleMod[];
    onCancel: () => void;
    onModOnly: () => void;
    onWithDependencies: () => void;
}

/** Asked when switching a mod would also leave its dependencies on or off. */
export function DependencyToggleModal({
    isOpen,
    enabling,
    mod,
    dependencies,
    onCancel,
    onModOnly,
    onWithDependencies,
}: DependencyToggleModalProps) {
    if (!isOpen) return null;

    const verb = enabling ? 'Enable' : 'Disable';

    return (
        <DependencyChoiceDialog
            title={`${verb} ${mod.name}`}
            subtitle={enabling
                ? 'It needs dependencies that are currently disabled.'
                : 'No other enabled mod needs these dependencies.'}
            mod={mod}
            listLabel={enabling ? 'Disabled dependencies' : 'Unused dependencies'}
            rows={dependencies}
            options={[
                {
                    id: 'with-dependencies',
                    label: `${verb} with dependencies`,
                    hint: enabling ? 'The mod will not work without them.' : 'Nothing else uses them.',
                    count: dependencies.length,
                    affects: () => true,
                },
                {
                    id: 'mod-only',
                    label: `${verb} mod only`,
                    hint: 'Leave the dependencies as they are.',
                    affects: () => false,
                },
            ]}
            defaultOptionId="with-dependencies"
            confirmLabel={verb}
            onConfirm={(id) => (id === 'mod-only' ? onModOnly() : onWithDependencies())}
            onCancel={onCancel}
        />
    );
}
