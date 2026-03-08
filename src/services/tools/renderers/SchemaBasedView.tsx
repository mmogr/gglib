import { JsonViewer } from './FallbackRenderer';
import { SortableTable } from '../../../components/ToolUI/SortableTable';
import type { JSONSchema, JSONSchemaProperty } from '../types';

export interface SchemaBasedViewProps {
  data: unknown;
  schema: Record<string, unknown>;
  level?: number;
}

const MAX_LEVEL = 2;

function isSafeUrl(value: unknown): value is string {
  return typeof value === 'string' && /^https?:\/\//i.test(value);
}

function renderPrimitive(data: unknown, schema: JSONSchemaProperty | JSONSchema): React.ReactNode {
  // date-time: try parsing as a date
  if (schema.type === 'string' && 'format' in schema && schema.format === 'date-time') {
    try {
      const d = new Date(String(data));
      if (!isNaN(d.getTime())) {
        return <span className="text-text font-mono">{d.toLocaleString()}</span>;
      }
    } catch {
      // fall through
    }
  }

  // uri: render as safe link (http/https only)
  if (schema.type === 'string' && 'format' in schema && schema.format === 'uri') {
    if (isSafeUrl(data)) {
      return (
        <a
          href={data}
          target="_blank"
          rel="noopener noreferrer"
          className="text-primary underline break-all"
        >
          {data}
        </a>
      );
    }
    return <span className="text-text font-mono">{String(data)}</span>;
  }

  // number with locale formatting
  if (schema.type === 'number' && typeof data === 'number') {
    return <span className="text-text font-mono">{data.toLocaleString()}</span>;
  }

  return <span className="text-text font-mono text-[12px]">{String(data ?? '')}</span>;
}

/**
 * Renders a tool result using its JSON Schema as a guide.
 * - Objects → two-column key/value table with recursive rendering (max 2 levels)
 * - Arrays of objects → SortableTable
 * - Strings with format "uri" → safe clickable link (http/https only)
 * - Strings with format "date-time" → toLocaleString
 * - Numbers → toLocaleString
 * - Deep nesting (level > MAX_LEVEL) → JsonViewer fallback
 */
export const SchemaBasedView: React.FC<SchemaBasedViewProps> = ({ data, schema, level = 0 }) => {
  // Depth guard: avoid runaway recursion on deeply nested schemas
  if (level > MAX_LEVEL) {
    return <JsonViewer data={data} label="Result" />;
  }

  // Arrays
  if (schema.type === 'array') {
    if (!Array.isArray(data)) {
      return <JsonViewer data={data} label="Result" />;
    }

    // Array of objects → SortableTable; infer column order from schema.items if available
    const firstItem = data[0];
    if (data.length > 0 && typeof firstItem === 'object' && firstItem !== null) {
      const schemaItems = schema.items as JSONSchemaProperty | undefined;
      const columns =
        schemaItems?.properties ? Object.keys(schemaItems.properties) : undefined;
      return <SortableTable rows={data as Record<string, unknown>[]} columns={columns} />;
    }

    // Array of primitives → simple list
    return (
      <ul className="list-none m-0 p-0 flex flex-col gap-0.5">
        {(data as unknown[]).map((item, i) => (
          <li key={i} className="font-mono text-[12px] text-text">
            {String(item ?? '')}
          </li>
        ))}
      </ul>
    );
  }

  // Objects
  if (schema.type === 'object') {
    const properties = (schema.properties ?? {}) as Record<string, JSONSchemaProperty>;
    const obj = (typeof data === 'object' && data !== null ? data : {}) as Record<string, unknown>;
    // Union schema keys with actual data keys so unknown keys still render
    const keys = Array.from(new Set([...Object.keys(properties), ...Object.keys(obj)]));

    if (keys.length === 0) {
      return <span className="text-text-muted italic text-[12px]">(empty)</span>;
    }

    return (
      <div className="overflow-x-auto rounded-lg border border-border my-1 text-[12px]">
        <table className="border-collapse w-full">
          <tbody>
            {keys.map((key) => {
              const propSchema = properties[key];
              const val = obj[key];
              return (
                <tr key={key} className="border-b border-border last:border-b-0 hover:bg-background-secondary transition-colors duration-75">
                  <td className="py-1.5 px-2.5 font-semibold text-text-secondary whitespace-nowrap border-r border-border w-1/3 align-top">
                    {key}
                  </td>
                  <td className="py-1.5 px-2.5 text-text align-top">
                    {propSchema ? (
                      <SchemaBasedView data={val} schema={propSchema as unknown as Record<string, unknown>} level={level + 1} />
                    ) : (
                      <span className="font-mono text-[12px]">{String(val ?? '')}</span>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    );
  }

  // Primitives (string, number, boolean, etc.)
  return renderPrimitive(data, schema as unknown as JSONSchemaProperty);
};

export default SchemaBasedView;
