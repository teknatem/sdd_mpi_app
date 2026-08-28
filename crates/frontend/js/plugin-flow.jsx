/**
 * PluginFlow — кит редактора графов для iframe плагина.
 *
 * Собирается в один IIFE (`static/plugin-flow.js` + `plugin-flow.css`) вместе с
 * React и @xyflow/react: см. `build:plugin-flow` в package.json. Автор плагина
 * React не пишет и JSX не видит — он зовёт `PluginFlow.render(...)`.
 *
 * **Кит ничего не знает про мост.** Ни `host`, ни `postMessage`, ни адрес
 * документа здесь не упоминаются: render принимает spec и колбэки, а чтение и
 * запись делает вызывающий. Это не стилистика — кит инфраструктурный, лежит в
 * static/ рядом с Chart.js и однажды понадобится вне iframe плагина (например,
 * штатной странице с тем же графом). Завяжи его на мост — и он туда не поедет.
 *
 * Тема — целиком в CSS (plugin-flow.css отображает токены приложения на --xy-*),
 * поэтому applyTheme() здесь почти пустой, в отличие от PluginCharts.
 */

import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  addEdge,
  applyEdgeChanges,
  applyNodeChanges,
  Background,
  Controls,
  Handle,
  MiniMap,
  Position,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
} from "@xyflow/react";
import { Graph, layout as dagreLayout } from "@dagrejs/dagre";

import "@xyflow/react/dist/style.css";
import "./plugin-flow.css";

const DEFAULT_NODE_WIDTH = 180;
const DEFAULT_NODE_HEIGHT = 52;

// ── Раскладка ───────────────────────────────────────────────────────────────

/**
 * Координаты по dagre.
 *
 * Нужна не для красоты: графы, сгенерированные LLM, приходят без позиций
 * вообще — без раскладки они лягут в одну точку.
 */
function layout(nodes, edges, options = {}) {
  const direction = options.direction || "TB";
  const nodeWidth = options.nodeWidth || DEFAULT_NODE_WIDTH;
  const nodeHeight = options.nodeHeight || DEFAULT_NODE_HEIGHT;

  const graph = new Graph({ multigraph: true });
  graph.setDefaultEdgeLabel(() => ({}));
  graph.setGraph({
    rankdir: direction,
    nodesep: options.nodeSep ?? 44,
    ranksep: options.rankSep ?? 64,
    marginx: 16,
    marginy: 16,
  });

  const known = new Set(nodes.map((node) => node.id));
  nodes.forEach((node) => {
    graph.setNode(node.id, {
      width: node.width || node.measured?.width || nodeWidth,
      height: node.height || node.measured?.height || nodeHeight,
    });
  });
  edges.forEach((edge, index) => {
    // Ребро в пустоту роняет dagre — пропускаем, валидация скажет о нём отдельно.
    if (known.has(edge.source) && known.has(edge.target)) {
      graph.setEdge(edge.source, edge.target, {}, edge.id || `e${index}`);
    }
  });

  dagreLayout(graph);

  return {
    nodes: nodes.map((node) => {
      const placed = graph.node(node.id);
      if (!placed) return node;
      const width = node.width || node.measured?.width || nodeWidth;
      const height = node.height || node.measured?.height || nodeHeight;
      return {
        ...node,
        position: { x: placed.x - width / 2, y: placed.y - height / 2 },
      };
    }),
    edges,
  };
}

function hasPosition(node) {
  return (
    node &&
    node.position &&
    Number.isFinite(node.position.x) &&
    Number.isFinite(node.position.y)
  );
}

// ── Валидация ───────────────────────────────────────────────────────────────

/**
 * Структурная проверка: дубли идентификаторов и рёбра в несуществующие узлы.
 *
 * Доменных правил (что с чем можно соединять в Процессе) здесь нет — это
 * отдельный слой; кит проверяет только то, что делает граф вообще связным.
 */
function validateSpec(spec) {
  const errors = [];
  const nodes = Array.isArray(spec?.nodes) ? spec.nodes : [];
  const edges = Array.isArray(spec?.edges) ? spec.edges : [];

  if (!Array.isArray(spec?.nodes)) errors.push("spec.nodes должен быть массивом");
  if (spec?.edges !== undefined && !Array.isArray(spec.edges)) {
    errors.push("spec.edges должен быть массивом");
  }

  const seen = new Set();
  nodes.forEach((node, index) => {
    if (!node || typeof node.id !== "string" || !node.id) {
      errors.push(`Узел #${index}: нужен непустой строковый id`);
      return;
    }
    if (seen.has(node.id)) errors.push(`Дубль идентификатора узла: ${node.id}`);
    seen.add(node.id);
  });

  edges.forEach((edge, index) => {
    if (!edge || typeof edge.source !== "string" || typeof edge.target !== "string") {
      errors.push(`Ребро #${index}: нужны source и target`);
      return;
    }
    if (!seen.has(edge.source)) errors.push(`Ребро #${index}: нет узла ${edge.source}`);
    if (!seen.has(edge.target)) errors.push(`Ребро #${index}: нет узла ${edge.target}`);
  });

  return { ok: errors.length === 0, errors };
}

function normalize(spec, autoLayout, onActivate) {
  const nodes = (Array.isArray(spec?.nodes) ? spec.nodes : []).map((node) => ({
    ...node,
    type: node.type || "editable",
    position: hasPosition(node) ? node.position : { x: 0, y: 0 },
    data: {
      ...(node.data || {}),
      label: node.data?.label ?? node.label ?? node.id,
      // Служебное поле: в serialize() не попадает, в файл не уезжает.
      __onActivate: onActivate,
    },
  }));
  const edges = (Array.isArray(spec?.edges) ? spec.edges : []).map((edge, index) => ({
    ...edge,
    id: edge.id || `e-${edge.source}-${edge.target}-${index}`,
  }));

  const needsLayout =
    autoLayout === "always" ||
    (autoLayout !== "never" &&
      nodes.length > 0 &&
      (Array.isArray(spec?.nodes) ? spec.nodes : []).some((node) => !hasPosition(node)));

  return needsLayout ? layout(nodes, edges) : { nodes, edges };
}

/** Наружу отдаём только доменные поля: рантайм-мусор xyflow в файл не пишем. */
function serialize(nodes, edges) {
  return {
    nodes: nodes.map((node) => {
      // __onActivate — функция хоста, а не данные графа: в сохраняемый JSON ей нельзя.
      const { __onActivate, ...data } = node.data || {};
      return {
        id: node.id,
        type: node.type,
        position: { x: Math.round(node.position.x), y: Math.round(node.position.y) },
        data,
      };
    }),
    edges: edges.map((edge) => ({
      id: edge.id,
      source: edge.source,
      target: edge.target,
      label: edge.label,
    })),
  };
}

// ── Узел ────────────────────────────────────────────────────────────────────

const EditableNode = memo(function EditableNode({ id, data, selected }) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(data.label ?? "");
  const { setNodes } = useReactFlow();
  // Что делает двойной клик, решает вызывающий: если он дал onNodeActivate —
  // узел «открывается» наружу (карточка, документ, что угодно), и кит про это
  // ничего не знает. Без колбэка остаётся правка подписи на месте.
  const activate = data.__onActivate;

  useEffect(() => {
    if (!editing) setDraft(data.label ?? "");
  }, [data.label, editing]);

  const commit = useCallback(() => {
    setEditing(false);
    const label = draft.trim() || id;
    if (label === data.label) return;
    setNodes((nodes) =>
      nodes.map((node) =>
        node.id === id ? { ...node, data: { ...node.data, label } } : node
      )
    );
  }, [draft, data.label, id, setNodes]);

  const className = selected
    ? "plugin-flow__node plugin-flow__node--selected"
    : "plugin-flow__node";

  return (
    <div
      className={className}
      title={activate ? "Двойной клик — открыть карточку" : "Двойной клик — переименовать"}
      onDoubleClick={(event) => {
        event.stopPropagation();
        if (activate) activate({ id, data });
        else setEditing(true);
      }}
    >
      <Handle type="target" position={Position.Top} />
      {data.kind ? <span className="plugin-flow__node-kind">{data.kind}</span> : null}
      {editing ? (
        <input
          className="plugin-flow__node-input"
          autoFocus
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key === "Enter") commit();
            if (event.key === "Escape") setEditing(false);
            event.stopPropagation();
          }}
        />
      ) : (
        <span>{data.label}</span>
      )}
      <Handle type="source" position={Position.Bottom} />
    </div>
  );
});

const NODE_TYPES = { editable: EditableNode };

// ── Полотно ─────────────────────────────────────────────────────────────────

// Выделение и обмер узлов правкой не считаются: иначе граф «пачкается» от
// одного клика, и грязный флаг перестаёт что-либо значить.
const COSMETIC_CHANGES = new Set(["select", "dimensions"]);

function isMeaningful(changes) {
  return changes.some((change) => !COSMETIC_CHANGES.has(change.type));
}

function FlowCanvas({ spec, options, api }) {
  const initial = useMemo(
    () => normalize(spec, options.autoLayout || "missing", options.onNodeActivate),
    // Пересборка только при новом spec: правки живут в состоянии ниже.
    [spec, options.autoLayout, options.onNodeActivate]
  );
  const [nodes, setNodes] = useState(initial.nodes);
  const [edges, setEdges] = useState(initial.edges);
  const [dirty, setDirty] = useState(false);
  const flow = useReactFlow();
  const latest = useRef({ nodes: initial.nodes, edges: initial.edges });
  const editable = options.editable !== false;

  useEffect(() => {
    latest.current = { nodes, edges };
  }, [nodes, edges]);

  // Колбэк дёргаем на переходе false→true, а не на каждое движение мыши:
  // за перетаскивание узла прилетают сотни change-событий.
  const markDirty = useCallback(() => {
    setDirty((was) => {
      if (!was && typeof options.onDirtyChange === "function") {
        options.onDirtyChange(true);
      }
      return true;
    });
  }, [options]);

  const onNodesChange = useCallback(
    (changes) => {
      setNodes((current) => applyNodeChanges(changes, current));
      if (isMeaningful(changes)) markDirty();
    },
    [markDirty]
  );

  const onEdgesChange = useCallback(
    (changes) => {
      setEdges((current) => applyEdgeChanges(changes, current));
      if (isMeaningful(changes)) markDirty();
    },
    [markDirty]
  );

  const onConnect = useCallback(
    (connection) => {
      setEdges((current) => addEdge({ ...connection }, current));
      markDirty();
    },
    [markDirty]
  );

  const addNode = useCallback(() => {
    const id = `n${Date.now().toString(36)}`;
    const center = flow.screenToFlowPosition
      ? flow.screenToFlowPosition({ x: 240, y: 160 })
      : { x: 80, y: 80 };
    setNodes((current) => [
      ...current,
      { id, type: "editable", position: center, data: { label: "Новый узел" } },
    ]);
    markDirty();
  }, [flow, markDirty]);

  const runLayout = useCallback(() => {
    const next = layout(latest.current.nodes, latest.current.edges, options.layout || {});
    setNodes(next.nodes);
    markDirty();
    requestAnimationFrame(() => flow.fitView({ padding: 0.15 }));
  }, [flow, markDirty, options.layout]);

  // Императивный контроллер для вызывающего: React снаружи не виден.
  useEffect(() => {
    api.getFlow = () => serialize(latest.current.nodes, latest.current.edges);
    api.setFlow = (next) => {
      const normalized = normalize(next, options.autoLayout || "missing", options.onNodeActivate);
      setNodes(normalized.nodes);
      setEdges(normalized.edges);
      setDirty(false);
      if (typeof options.onDirtyChange === "function") options.onDirtyChange(false);
      requestAnimationFrame(() => flow.fitView({ padding: 0.15 }));
    };
    api.isDirty = () => dirty;
    api.markSaved = () => {
      setDirty(false);
      if (typeof options.onDirtyChange === "function") options.onDirtyChange(false);
    };
    api.autoLayout = runLayout;
    api.fitView = () => flow.fitView({ padding: 0.15 });
  }, [api, dirty, flow, options, runLayout]);

  return (
    <div className="plugin-flow">
      {editable && options.toolbar !== false ? (
        <div className="plugin-flow__toolbar">
          <button type="button" className="btn btn--secondary" onClick={addNode}>
            Добавить узел
          </button>
          <button type="button" className="btn btn--ghost" onClick={runLayout}>
            Разложить
          </button>
          <button
            type="button"
            className="btn btn--ghost"
            onClick={() => flow.fitView({ padding: 0.15 })}
          >
            Вписать
          </button>
          <span className="plugin-flow__hint">
            {options.onNodeActivate ? "Двойной клик — открыть карточку" : "Двойной клик — переименовать"}
            {" · Delete — удалить выделенное"}
          </span>
        </div>
      ) : null}
      <div className="plugin-flow__canvas">
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={NODE_TYPES}
          onNodesChange={editable ? onNodesChange : undefined}
          onEdgesChange={editable ? onEdgesChange : undefined}
          onConnect={editable ? onConnect : undefined}
          nodesDraggable={editable}
          nodesConnectable={editable}
          elementsSelectable
          deleteKeyCode={editable ? ["Delete", "Backspace"] : null}
          fitView
          proOptions={{ hideAttribution: false }}
        >
          <Background gap={16} />
          <Controls showInteractive={false} />
          {options.minimap === false ? null : <MiniMap pannable zoomable />}
        </ReactFlow>
      </div>
    </div>
  );
}

// ── Публичный API ───────────────────────────────────────────────────────────

const live = new Set();

function render(target, spec, options = {}) {
  const container =
    typeof target === "string" ? document.querySelector(target) : target;
  if (!container) throw new Error("PluginFlow.render: контейнер не найден");

  const report = validateSpec(spec);
  if (!report.ok && options.strict) {
    throw new Error("PluginFlow.render: " + report.errors.join("; "));
  }

  const root = createRoot(container);
  const api = {};
  root.render(
    <ReactFlowProvider>
      <FlowCanvas spec={spec} options={options} api={api} />
    </ReactFlowProvider>
  );

  const controller = {
    getFlow: () => (api.getFlow ? api.getFlow() : { nodes: [], edges: [] }),
    setFlow: (next) => api.setFlow && api.setFlow(next),
    isDirty: () => (api.isDirty ? api.isDirty() : false),
    markSaved: () => api.markSaved && api.markSaved(),
    autoLayout: () => api.autoLayout && api.autoLayout(),
    fitView: () => api.fitView && api.fitView(),
    validation: report,
    destroy() {
      live.delete(controller);
      // Асинхронно: React запрещает unmount во время рендера родителя.
      queueMicrotask(() => root.unmount());
    },
  };
  live.add(controller);
  return controller;
}

/**
 * Заглушка ради симметрии с PluginCharts/PluginTables.
 *
 * Цвета берутся из CSS-переменных, а те пересчитываются каскадом при подмене
 * <link> темы, — перекрашивать вручную нечего.
 */
function applyTheme() {}

function destroyAll() {
  for (const controller of Array.from(live)) controller.destroy();
}

if (!window.PluginFlow) {
  window.PluginFlow = { render, layout, validateSpec, applyTheme, destroyAll };
}
