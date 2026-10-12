import { isValidElement, type ReactElement, type ReactNode } from "react";

type Props = Record<string, unknown> & { children?: ReactNode };
type Component = (props: Props) => ReactNode;

/** The host elements under `node`, in order; it calls each component, so none may use hooks. */
export function hostElements(node: ReactNode): ReactElement<Props>[] {
  if (Array.isArray(node)) return node.flatMap(hostElements);
  if (!isValidElement<Props>(node)) return [];
  const type: unknown = node.type;
  if (typeof type === "function")
    return hostElements((type as Component)(node.props));
  if (typeof type === "object" && type !== null && "type" in type)
    return hostElements((type.type as Component)(node.props));
  const children = hostElements(node.props.children);
  return typeof type === "string" ? [node, ...children] : children;
}

/** The host element whose text or `aria-label` is `name`. */
export function byName(node: ReactNode, name: string): ReactElement<Props> {
  const found = hostElements(node).find(
    (element) => element.props["aria-label"] === name || text(element) === name,
  );
  if (!found) throw new Error(`no element named ${name}`);
  return found;
}

function text(element: ReactElement<Props>): string {
  const children = element.props.children;
  return typeof children === "string" ? children : "";
}
