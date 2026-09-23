import { useState, useRef, useEffect, ReactNode } from 'react';

export type ActionMenuItem = {
  id: string;
  label: string;
  hint?: string;
  badge?: string | number;
  disabled?: boolean;
  danger?: boolean;
  highlight?: boolean;
  onClick: () => void;
};

export type DividerMenuItem = { type: 'divider' };
export type HeaderMenuItem = { type: 'header'; label: string };
export type CheckboxMenuItem = {
  type: 'checkbox';
  id: string;
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
};

export type MenuItem = ActionMenuItem | DividerMenuItem | HeaderMenuItem | CheckboxMenuItem;

export type SplitAction = {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  title?: string;
};

type Props = {
  label?: ReactNode;
  title?: string;
  variant?: 'primary' | 'ghost' | 'secondary';
  splitAction?: SplitAction;
  items: MenuItem[];
  align?: 'left' | 'right';
  className?: string;
};

export function MenuDropdown({
  label,
  title,
  variant = 'ghost',
  splitAction,
  items,
  align = 'right',
  className = '',
}: Props) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handlePointerDown = (e: MouseEvent | TouchEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };

    document.addEventListener('mousedown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('mousedown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [open]);

  const toggle = () => setOpen((prev) => !prev);

  return (
    <div ref={containerRef} className={`menu-dropdown ${className}`}>
      {splitAction ? (
        <div className="split-button">
          <button
            type="button"
            className="primary split-main"
            title={splitAction.title}
            disabled={splitAction.disabled}
            onClick={splitAction.onClick}
          >
            {splitAction.label}
          </button>
          <button
            type="button"
            className={`primary split-toggle ${open ? 'active' : ''}`}
            aria-haspopup="true"
            aria-expanded={open}
            title={title || 'More actions'}
            onClick={toggle}
          >
            ▾
          </button>
        </div>
      ) : (
        <button
          type="button"
          className={`${variant} menu-trigger ${open ? 'active' : ''}`}
          aria-haspopup="true"
          aria-expanded={open}
          title={title}
          onClick={toggle}
        >
          {label}
          <span className="menu-caret">▾</span>
        </button>
      )}

      {open && (
        <div className={`menu-popover align-${align}`} role="menu">
          {items.map((item, idx) => {
            if ('type' in item) {
              if (item.type === 'divider') {
                return <div key={`div-${idx}`} className="menu-divider" role="separator" />;
              }
              if (item.type === 'header') {
                return (
                  <div key={`hdr-${idx}`} className="menu-header">
                    {item.label}
                  </div>
                );
              }
              if (item.type === 'checkbox') {
                return (
                  <label key={item.id} className="menu-checkbox-item">
                    <input
                      type="checkbox"
                      checked={item.checked}
                      onChange={(e) => item.onChange(e.target.checked)}
                    />
                    <div className="menu-item-text">
                      <span className="menu-item-title">{item.label}</span>
                      {item.hint && <small className="menu-item-hint">{item.hint}</small>}
                    </div>
                  </label>
                );
              }
            }

            const actionItem = item as ActionMenuItem;
            return (
              <button
                key={actionItem.id}
                type="button"
                role="menuitem"
                disabled={actionItem.disabled}
                className={`menu-item ${actionItem.danger ? 'danger' : ''} ${actionItem.highlight ? 'highlight' : ''}`}
                onClick={() => {
                  setOpen(false);
                  actionItem.onClick();
                }}
              >
                <div className="menu-item-text">
                  <span className="menu-item-title">{actionItem.label}</span>
                  {actionItem.hint && <small className="menu-item-hint">{actionItem.hint}</small>}
                </div>
                {actionItem.badge != null && (
                  <span className="menu-item-badge">{actionItem.badge}</span>
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
