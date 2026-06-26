const API_BASE = '/api';
let currentTable = null;
let currentMetadata = null;
let currentDomain = null;
let currentDomainInfo = null;
let activeQueryPlan = null;
let activePredicates = {};

// On Load
document.addEventListener('DOMContentLoaded', () => {
  initTabs();
  initModals();
  loadCatalogTree();

  // Bind Forms
  document.getElementById('queryForm').addEventListener('submit', handleQuerySubmit);
  document.getElementById('appendForm').addEventListener('submit', handleAppendSubmit);
  document.getElementById('compactForm').addEventListener('submit', handleCompactSubmit);
  document.getElementById('deleteForm').addEventListener('submit', handleDeleteSubmit);
  document.getElementById('createDomainForm').addEventListener('submit', handleCreateDomainSubmit);
  document.getElementById('addRuleForm').addEventListener('submit', handleAddRuleSubmit);
  document.getElementById('addRelationForm').addEventListener('submit', handleAddRelationSubmit);

  // Bind Rule constraint selector toggle
  document.getElementById('ruleConstraintType').addEventListener('change', (e) => {
    const configGroup = document.getElementById('ruleConfigGroup');
    const label = document.getElementById('lblRuleConfig');
    const input = document.getElementById('ruleConfigVal');
    
    if (e.target.value === 'NotNull') {
      configGroup.style.display = 'none';
      input.required = false;
    } else {
      configGroup.style.display = 'block';
      input.required = true;
      if (e.target.value === 'MinMax') {
        label.textContent = 'Bounds (min,max - e.g. 0,100)';
        input.placeholder = 'e.g. 0,100';
      } else if (e.target.value === 'AllowedValues') {
        label.textContent = 'Allowed Values (comma separated)';
        input.placeholder = 'e.g. VN,US,SG';
      } else if (e.target.value === 'RegexMatch') {
        label.textContent = 'Regex Pattern';
        input.placeholder = 'e.g. ^[0-9]+$';
      }
    }
  });

  // Bind Layout Optimizer
  document.getElementById('btnRecommendLayout').addEventListener('click', () => optimizeLayout(true));
  document.getElementById('btnOptimizeLayout').addEventListener('click', () => optimizeLayout(false));
  document.getElementById('btnResetGraphView').addEventListener('click', () => {
    if (currentMetadata) renderGraph(currentMetadata);
  });
});

// --- UI Navigation (Tabs & Modals) ---

function initTabs() {
  const tabs = document.querySelectorAll('.tab-btn');
  tabs.forEach(tab => {
    tab.addEventListener('click', () => {
      tabs.forEach(t => t.classList.remove('active'));
      tab.classList.add('active');

      const targetView = tab.getAttribute('data-tab');
      document.querySelectorAll('.tab-view').forEach(view => {
        view.classList.remove('active');
      });
      document.getElementById(targetView).classList.add('active');
    });
  });
}

function initModals() {
  const tableModal = document.getElementById('createTableModal');
  const domainModal = document.getElementById('createDomainModal');
  const ruleModal = document.getElementById('addRuleModal');
  const relationModal = document.getElementById('addRelationModal');

  document.getElementById('btnNewTable').onclick = () => tableModal.style.display = 'block';
  document.querySelector('#createTableModal .close-modal').onclick = () => tableModal.style.display = 'none';

  document.getElementById('btnNewDomain').onclick = () => domainModal.style.display = 'block';
  document.querySelector('.close-domain-modal').onclick = () => domainModal.style.display = 'none';

  document.getElementById('btnAddDomainRule').onclick = () => ruleModal.style.display = 'block';
  document.querySelector('.close-rule-modal').onclick = () => ruleModal.style.display = 'none';

  document.getElementById('btnAddDomainRelation').onclick = () => relationModal.style.display = 'block';
  document.querySelector('.close-relation-modal').onclick = () => relationModal.style.display = 'none';

  window.onclick = (e) => {
    if (e.target === tableModal) tableModal.style.display = 'none';
    if (e.target === domainModal) domainModal.style.display = 'none';
    if (e.target === ruleModal) ruleModal.style.display = 'none';
    if (e.target === relationModal) relationModal.style.display = 'none';
  };

  // Schema setup row additions
  document.getElementById('btnAddModalField').addEventListener('click', () => {
    const tbody = document.getElementById('modalSchemaBody');
    const tr = document.createElement('tr');
    tr.innerHTML = `
      <td><input type="text" class="form-control field-name" placeholder="field_name" required></td>
      <td>
        <select class="form-control field-type">
          <option value="string">String</option>
          <option value="int">Int</option>
          <option value="long">Long</option>
          <option value="boolean">Boolean</option>
          <option value="float">Float</option>
          <option value="double">Double</option>
        </select>
      </td>
      <td><input type="checkbox" class="field-required"></td>
      <td><button type="button" class="close-modal remove-row">&times;</button></td>
    `;
    tbody.appendChild(tr);
    tr.querySelector('.remove-row').addEventListener('click', () => tr.remove());
  });

  // Dimension additions
  document.getElementById('btnAddModalDim').addEventListener('click', () => {
    const list = document.getElementById('modalDimsList');
    const div = document.createElement('div');
    div.className = 'dim-row';
    div.innerHTML = `
      <input type="text" class="form-control dim-input" placeholder="Dimension name" required>
      <button type="button" class="close-modal remove-dim" style="margin-left: 8px; line-height: 1.8;">&times;</button>
    `;
    list.appendChild(div);
    div.querySelector('.remove-dim').addEventListener('click', () => div.remove());
  });

  // Handle Form Submit
  document.getElementById('createTableForm').addEventListener('submit', handleCreateTableSubmit);
}

// --- API Calls ---

async function loadCatalogTree(selectPath = null) {
  try {
    const response = await fetch(`${API_BASE}/catalog/tree`);
    const tree = await response.json();
    const rootUl = document.getElementById('treeRoot');
    rootUl.innerHTML = '';

    renderTreeNode(tree, rootUl, 0);

    // If we want to auto-select something
    if (selectPath) {
      const activeEl = rootUl.querySelector(`[data-path="${selectPath}"]`);
      if (activeEl) {
        activeEl.click();
      }
    } else {
      // Auto select the first domain or table
      const firstNode = rootUl.querySelector('.tree-node-item[data-type="domain"], .tree-node-item[data-type="table"]');
      if (firstNode) {
        firstNode.click();
      }
    }
  } catch (e) {
    showToast(`Error loading catalog tree: ${e.message}`, 'error');
  }
}

function renderTreeNode(node, parentElement, depth) {
  if (!node.name) return;
  const li = document.createElement('li');
  li.className = 'tree-node';

  const itemDiv = document.createElement('div');
  itemDiv.className = 'tree-node-item';
  itemDiv.setAttribute('data-path', node.path);
  itemDiv.setAttribute('data-type', node.node_type);
  itemDiv.style.paddingLeft = `${depth * 12 + 6}px`;

  // Icon depending on type
  let iconSvg = '';
  if (node.node_type === 'root') {
    iconSvg = `<svg class="tree-node-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="stroke: var(--text-muted);"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>`;
  } else if (node.node_type === 'domain') {
    iconSvg = `<svg class="tree-node-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="stroke: var(--accent-purple);"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>`;
  } else if (node.node_type === 'table') {
    iconSvg = `<svg class="tree-node-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="stroke: var(--accent-blue);"><ellipse cx="12" cy="5" rx="9" ry="3"></ellipse><path d="M3 5v14c0 1.66 4 3 9 3s9-1.34 9-3V5"></path><path d="M3 12c0 1.66 4 3 9 3s9-1.34 9-3"></path></svg>`;
  } else {
    iconSvg = `<svg class="tree-node-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="stroke: var(--text-muted);"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>`;
  }

  const labelSpan = document.createElement('span');
  labelSpan.textContent = node.name;
  
  itemDiv.innerHTML = iconSvg;
  itemDiv.appendChild(labelSpan);
  li.appendChild(itemDiv);

  if (node.node_type === 'root' || node.node_type === 'domain' || node.node_type === 'folder') {
    if (node.children && node.children.length > 0) {
      const childUl = document.createElement('ul');
      childUl.className = 'tree-node-children';
      node.children.forEach(child => {
        renderTreeNode(child, childUl, depth + 1);
      });
      li.appendChild(childUl);
    }
  }

  itemDiv.addEventListener('click', (e) => {
    e.stopPropagation();
    document.querySelectorAll('.tree-node-item').forEach(el => el.classList.remove('active'));
    itemDiv.classList.add('active');

    if (node.node_type === 'domain') {
      selectDomain(node.path);
    } else if (node.node_type === 'table') {
      selectTable(node.path);
    }
  });

  parentElement.appendChild(li);
}

async function selectDomain(name) {
  currentDomain = name;
  currentTable = null;
  currentMetadata = null;

  try {
    const response = await fetch(`${API_BASE}/domain/${name}`);
    const info = await response.json();
    currentDomainInfo = info;

    // Show domain tab button and activate it
    const domainBtn = document.getElementById('tabDomainBtn');
    domainBtn.style.display = 'block';
    
    document.querySelectorAll('.tab-btn').forEach(t => t.classList.remove('active'));
    domainBtn.classList.add('active');

    document.querySelectorAll('.tab-view').forEach(v => v.classList.remove('active'));
    const domainTab = document.getElementById('tabDomain');
    domainTab.classList.add('active');

    // Update Header
    document.getElementById('currentTableName').textContent = `Domain: ${info.name}`;
    document.getElementById('currentTablePath').textContent = `catalog://${info.name}`;

    // Render details
    document.getElementById('lblDomainName').textContent = `Domain: ${info.name}`;
    document.getElementById('lblDomainDesc').textContent = info.description || 'No description provided.';

    renderDomainRules(info);
    renderDomainRelations(info);
    fetchErMetadataAndDraw(info);

  } catch (e) {
    showToast(`Error inspecting domain: ${e.message}`, 'error');
  }
}

function renderDomainRules(info) {
  const tbody = document.getElementById('domainRulesBody');
  tbody.innerHTML = '';

  if (info.rules.length === 0) {
    tbody.innerHTML = `<tr><td colspan="4" class="empty-state" style="text-align: center; padding: 12px; color: var(--text-muted);">No rules defined for this domain.</td></tr>`;
    return;
  }

  info.rules.forEach((rule, idx) => {
    const tr = document.createElement('tr');
    
    let constraintType = rule.constraint.type;
    let params = '-';
    
    if (rule.constraint.type === 'MinMax') {
      const min = rule.constraint.config.min !== null ? rule.constraint.config.min : '';
      const max = rule.constraint.config.max !== null ? rule.constraint.config.max : '';
      params = `[${min} .. ${max}]`;
    } else if (rule.constraint.type === 'AllowedValues') {
      params = rule.constraint.config.join(', ');
    } else if (rule.constraint.type === 'RegexMatch') {
      params = rule.constraint.config;
    }

    tr.innerHTML = `
      <td style="padding: 10px 8px; font-weight: 500;">${rule.column_name}</td>
      <td style="padding: 10px 8px;"><span class="rule-badge">${constraintType}</span></td>
      <td style="padding: 10px 8px; font-family: var(--font-mono); font-size: 0.8rem;" title="${params}">${params}</td>
      <td style="padding: 10px 8px; text-align: right;">
        <button class="icon-btn remove-rule-btn" data-index="${idx}" title="Remove Rule" style="color: var(--accent-red); background: none; border: none; cursor: pointer; font-size: 1.1rem;">&times;</button>
      </td>
    `;
    
    tr.querySelector('.remove-rule-btn').onclick = (e) => {
      e.stopPropagation();
      removeDomainRule(idx);
    };

    tbody.appendChild(tr);
  });
}

function renderDomainRelations(info) {
  const tbody = document.getElementById('domainRelationsBody');
  tbody.innerHTML = '';

  if (info.relations.length === 0) {
    tbody.innerHTML = `<tr><td colspan="4" class="empty-state" style="text-align: center; padding: 12px; color: var(--text-muted);">No relationships defined.</td></tr>`;
    return;
  }

  info.relations.forEach((rel, idx) => {
    const tr = document.createElement('tr');
    tr.innerHTML = `
      <td style="padding: 10px 8px; font-family: var(--font-mono);">${rel.from_table}.${rel.from_column}</td>
      <td style="padding: 10px 8px; text-align: center; color: var(--accent-purple); font-weight: 500;">➔ Joins ➔</td>
      <td style="padding: 10px 8px; font-family: var(--font-mono);">${rel.to_table}.${rel.to_column}</td>
      <td style="padding: 10px 8px; text-align: right;">
        <button class="icon-btn remove-relation-btn" data-index="${idx}" title="Remove Relation" style="color: var(--accent-red); background: none; border: none; cursor: pointer; font-size: 1.1rem;">&times;</button>
      </td>
    `;

    tr.querySelector('.remove-relation-btn').onclick = (e) => {
      e.stopPropagation();
      removeDomainRelation(idx);
    };

    tbody.appendChild(tr);
  });
}

async function removeDomainRule(idx) {
  if (!confirm("Are you sure you want to remove this rule?")) return;
  const rules = [...currentDomainInfo.rules];
  rules.splice(idx, 1);
  await saveDomain(rules, currentDomainInfo.relations);
}

async function removeDomainRelation(idx) {
  if (!confirm("Are you sure you want to remove this relation?")) return;
  const relations = [...currentDomainInfo.relations];
  relations.splice(idx, 1);
  await saveDomain(currentDomainInfo.rules, relations);
}

async function saveDomain(rules, relations) {
  try {
    const response = await fetch(`${API_BASE}/domain/create`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        name: currentDomainInfo.name,
        description: currentDomainInfo.description,
        rules,
        relations
      })
    });
    if (response.ok) {
      showToast("Domain updated successfully.", "success");
      selectDomain(currentDomainInfo.name);
    } else {
      const err = await response.json();
      showToast(`Error updating domain: ${err.error}`, "error");
    }
  } catch (e) {
    showToast(`Error saving domain: ${e.message}`, 'error');
  }
}

async function fetchErMetadataAndDraw(domainInfo) {
  const svg = document.getElementById('erSvg');
  const linksGroup = document.getElementById('erLinksGroup');
  const nodesGroup = document.getElementById('erNodesGroup');

  linksGroup.innerHTML = '';
  nodesGroup.innerHTML = '';

  const tableDataPromises = domainInfo.tables.map(async t => {
    try {
      const res = await fetch(`${API_BASE}/table/${t}`);
      return await res.json();
    } catch {
      return null;
    }
  });
  const tablesInfo = (await Promise.all(tableDataPromises)).filter(t => t !== null);

  if (tablesInfo.length === 0) {
    const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
    text.setAttribute('x', '50%');
    text.setAttribute('y', '50%');
    text.setAttribute('text-anchor', 'middle');
    text.setAttribute('fill', 'var(--text-muted)');
    text.setAttribute('font-family', 'var(--font-title)');
    text.textContent = 'No tables registered in this domain yet.';
    nodesGroup.appendChild(text);
    return;
  }

  // Grid layout positioning
  const boxWidth = 200;
  const colHeight = 22;
  const headerHeight = 35;
  const positions = {};

  tablesInfo.forEach((tInfo, idx) => {
    const colIdx = idx % 2;
    const rowIdx = Math.floor(idx / 2);
    const x = colIdx * 280 + 50;
    const y = rowIdx * 200 + 45;

    positions[tInfo.name] = { x, y, info: tInfo };
  });

  // 1. Draw table nodes
  tablesInfo.forEach(tInfo => {
    const pos = positions[tInfo.name];
    const columns = tInfo.schema.fields || [];
    const height = headerHeight + columns.length * colHeight + 8;

    // Table Box Group
    const g = document.createElementNS('http://www.w3.org/2000/svg', 'g');
    g.setAttribute('transform', `translate(${pos.x}, ${pos.y})`);

    // Outer rect
    const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
    rect.setAttribute('width', boxWidth);
    rect.setAttribute('height', height);
    rect.setAttribute('class', 'er-table-box');
    g.appendChild(rect);

    // Header rect
    const headerRect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
    headerRect.setAttribute('width', boxWidth);
    headerRect.setAttribute('height', headerHeight);
    headerRect.setAttribute('class', 'er-table-header');
    headerRect.setAttribute('rx', '6');
    headerRect.setAttribute('ry', '6');
    g.appendChild(headerRect);

    // Header text
    const titleText = document.createElementNS('http://www.w3.org/2000/svg', 'text');
    titleText.setAttribute('x', '15');
    titleText.setAttribute('y', '22');
    titleText.setAttribute('class', 'er-table-title');
    const displayName = tInfo.name.includes('.') ? tInfo.name.split('.').pop() : tInfo.name;
    titleText.textContent = displayName;
    g.appendChild(titleText);

    // Columns list
    columns.forEach((col, cIdx) => {
      const colY = headerHeight + (cIdx * colHeight) + 16;
      
      const colText = document.createElementNS('http://www.w3.org/2000/svg', 'text');
      colText.setAttribute('x', '15');
      colText.setAttribute('y', colY);
      colText.setAttribute('class', 'er-column-text');
      
      const isRelationKey = domainInfo.relations.some(r => 
        (r.from_table === tInfo.name && r.from_column === col.name) ||
        (r.to_table === tInfo.name && r.to_column === col.name)
      );

      if (isRelationKey) {
        colText.setAttribute('class', 'er-column-text er-column-key');
        colText.innerHTML = `🔑 ${col.name} <tspan fill="rgba(255,255,255,0.3)">:${col.field_type}</tspan>`;
      } else {
        colText.innerHTML = `${col.name} <tspan fill="rgba(255,255,255,0.35)">:${col.field_type}</tspan>`;
      }
      g.appendChild(colText);
    });

    nodesGroup.appendChild(g);
  });

  // 2. Draw relationship connection lines
  domainInfo.relations.forEach(rel => {
    const fromPos = positions[rel.from_table];
    const toPos = positions[rel.to_table];

    if (!fromPos || !toPos) return;

    const fromCols = fromPos.info.schema.fields || [];
    const toCols = toPos.info.schema.fields || [];

    const fromColIdx = fromCols.findIndex(c => c.name === rel.from_column);
    const toColIdx = toCols.findIndex(c => c.name === rel.to_column);

    if (fromColIdx === -1 || toColIdx === -1) return;

    const fromX = fromPos.x + boxWidth;
    const fromY = fromPos.y + headerHeight + (fromColIdx * colHeight) + 10;

    const toX = toPos.x;
    const toY = toPos.y + headerHeight + (toColIdx * colHeight) + 10;

    const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    const dx = Math.abs(toX - fromX) * 0.5;
    const d = `M ${fromX} ${fromY} C ${fromX + dx} ${fromY}, ${toX - dx} ${toY}, ${toX} ${toY}`;
    
    path.setAttribute('d', d);
    path.setAttribute('class', 'er-relation-line');
    linksGroup.appendChild(path);

    const dot1 = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    dot1.setAttribute('cx', fromX);
    dot1.setAttribute('cy', fromY);
    dot1.setAttribute('r', '4');
    dot1.setAttribute('fill', 'var(--accent-purple)');
    linksGroup.appendChild(dot1);

    const dot2 = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    dot2.setAttribute('cx', toX);
    dot2.setAttribute('cy', toY);
    dot2.setAttribute('r', '4');
    dot2.setAttribute('fill', 'var(--accent-purple)');
    linksGroup.appendChild(dot2);
  });
}

async function handleCreateDomainSubmit(e) {
  e.preventDefault();
  const name = document.getElementById('newDomainName').value;
  const description = document.getElementById('newDomainDesc').value;

  try {
    const response = await fetch(`${API_BASE}/domain/create`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        name,
        description,
        rules: [],
        relations: []
      })
    });

    if (response.ok) {
      showToast(`Domain '${name}' created successfully.`, 'success');
      document.getElementById('createDomainModal').style.display = 'none';
      document.getElementById('createDomainForm').reset();
      loadCatalogTree(name);
    } else {
      const err = await response.json();
      showToast(`Error creating domain: ${err.error}`, 'error');
    }
  } catch (err) {
    showToast(`Error creating domain: ${err.message}`, 'error');
  }
}

async function handleAddRuleSubmit(e) {
  e.preventDefault();
  const col = document.getElementById('ruleColumnName').value;
  const type = document.getElementById('ruleConstraintType').value;
  const configVal = document.getElementById('ruleConfigVal').value;

  let constraint = { type, config: null };

  if (type === 'MinMax') {
    const parts = configVal.split(',');
    constraint.config = {
      min: parts[0] ? parts[0].trim() : null,
      max: parts[1] ? parts[1].trim() : null
    };
  } else if (type === 'AllowedValues') {
    constraint.config = configVal.split(',').map(s => s.trim());
  } else if (type === 'RegexMatch') {
    constraint.config = configVal;
  }

  const rules = [...currentDomainInfo.rules, { column_name: col, constraint }];
  
  await saveDomain(rules, currentDomainInfo.relations);
  document.getElementById('addRuleModal').style.display = 'none';
  document.getElementById('addRuleForm').reset();
  document.getElementById('ruleConfigGroup').style.display = 'none';
}

async function handleAddRelationSubmit(e) {
  e.preventDefault();
  const from = document.getElementById('relFromTable').value;
  const to = document.getElementById('relToTable').value;

  const fromParts = from.split('.');
  const toParts = to.split('.');

  if (fromParts.length < 2 || toParts.length < 2) {
    showToast("Invalid format. Use: table.column", "error");
    return;
  }

  const relation = {
    from_table: fromParts.slice(0, -1).join('.'),
    from_column: fromParts[fromParts.length - 1],
    to_table: toParts.slice(0, -1).join('.'),
    to_column: toParts[toParts.length - 1],
  };

  const relations = [...currentDomainInfo.relations, relation];

  await saveDomain(currentDomainInfo.rules, relations);
  document.getElementById('addRelationModal').style.display = 'none';
  document.getElementById('addRelationForm').reset();
}

async function selectTable(name) {
  currentTable = name;
  currentDomain = null;
  activeQueryPlan = null;
  activePredicates = {};

  // Hide domain tab and activate Graph tab
  document.getElementById('tabDomainBtn').style.display = 'none';
  document.querySelectorAll('.tab-btn').forEach(t => t.classList.remove('active'));
  document.getElementById('tabGraphBtn').classList.add('active');

  document.querySelectorAll('.tab-view').forEach(v => v.classList.remove('active'));
  document.getElementById('tabGraph').classList.add('active');

  try {
    const response = await fetch(`${API_BASE}/table/${name}`);
    const info = await response.json();
    currentMetadata = info;

    // Update Header
    document.getElementById('currentTableName').textContent = info.name;
    document.getElementById('currentTablePath').textContent = `warehouse://${info.path}`;

    // Update Stats Card
    document.getElementById('valSnapshotId').textContent = info.current_snapshot_id !== null ? info.current_snapshot_id : 'None';
    document.getElementById('valRecords').textContent = info.record_count.toLocaleString();
    document.getElementById('valPhysicalFiles').textContent = info.physical_file_count;
    document.getElementById('valVirtualFiles').textContent = info.virtual_file_count;

    // Update dimensions badge
    document.getElementById('activeDimensionsBadge').textContent = info.graph_dimensions.join(' ➔ ');

    // Render Graph
    renderGraph(info);

    // Render Registry
    renderRegistry(info);

    // Build Query inputs
    buildQueryInputs(info);

    // Build Append inputs
    buildAppendInputs(info);

    // Build Compaction inputs
    buildCompactionInputs(info);

    // Build Deletes dropdown
    buildDeleteDropdown(info);

  } catch (e) {
    showToast(`Error inspecting table: ${e.message}`, 'error');
  }
}

// --- DOM Generators ---

function buildQueryInputs(info) {
  const eqContainer = document.getElementById('equalityPredicatesContainer');
  const rContainer = document.getElementById('rangePredicatesContainer');

  eqContainer.innerHTML = '';
  rContainer.innerHTML = '';

  info.graph_dimensions.forEach(dim => {
    // Equality Predicates
    const eqDiv = document.createElement('div');
    eqDiv.className = 'form-group';
    eqDiv.innerHTML = `
      <label for="eq_${dim}">${dim}</label>
      <input type="text" id="eq_${dim}" class="form-control" placeholder="Any">
    `;
    eqContainer.appendChild(eqDiv);

    // Range Predicates
    const rDiv = document.createElement('div');
    rDiv.className = 'form-group';
    rDiv.innerHTML = `
      <label>${dim} range</label>
      <div class="range-inputs">
        <input type="text" id="range_start_${dim}" class="form-control" placeholder="Start">
        <span>..</span>
        <input type="text" id="range_end_${dim}" class="form-control" placeholder="End">
      </div>
    `;
    rContainer.appendChild(rDiv);
  });
}

function buildAppendInputs(info) {
  const container = document.getElementById('appendPartitionsContainer');
  container.innerHTML = '';

  info.graph_dimensions.forEach(dim => {
    const div = document.createElement('div');
    div.className = 'form-group';
    div.innerHTML = `
      <label for="append_part_${dim}">${dim} value</label>
      <input type="text" id="append_part_${dim}" class="form-control" placeholder="e.g., VN" required>
    `;
    container.appendChild(div);
  });
}

function buildCompactionInputs(info) {
  const container = document.getElementById('compactPartitionsContainer');
  container.innerHTML = '';

  info.graph_dimensions.forEach(dim => {
    const div = document.createElement('div');
    div.className = 'form-group';
    div.innerHTML = `
      <label for="compact_part_${dim}">${dim} value</label>
      <input type="text" id="compact_part_${dim}" class="form-control" placeholder="e.g., VN" required>
    `;
    container.appendChild(div);
  });
}

function buildDeleteDropdown(info) {
  const select = document.getElementById('deleteFilePath');
  select.innerHTML = '<option value="">Select an active file...</option>';

  const addedFiles = new Set();
  Object.values(info.virtual_files).forEach(vf => {
    vf.physical_files.forEach(file => {
      if (!addedFiles.has(file)) {
        addedFiles.add(file);
        const opt = document.createElement('option');
        opt.value = file;
        opt.textContent = file;
        select.appendChild(opt);
      }
    });
  });
}

function renderRegistry(info) {
  const registry = document.getElementById('virtualFileRegistry');
  registry.innerHTML = '';

  const virtualFiles = Object.values(info.virtual_files);
  if (virtualFiles.length === 0) {
    registry.innerHTML = '<div class="empty-state">No virtual files registered.</div>';
    return;
  }

  // Render list descending
  virtualFiles.reverse().forEach(vf => {
    const item = document.createElement('div');
    item.className = 'vf-item';

    let physicalHtml = '';
    vf.physical_files.forEach(file => {
      // Find stats or delete bitmap
      physicalHtml += `
        <li class="vf-physical-item">
          <span class="file-path">${file}</span>
        </li>
      `;
    });

    item.innerHTML = `
      <div class="vf-item-header">
        <span class="vf-id">${vf.id}</span>
        <span class="vf-records">${vf.record_count.toLocaleString()} rows</span>
      </div>
      <ul class="vf-physical-list">
        ${physicalHtml}
      </ul>
    `;
    registry.appendChild(item);
  });
}

// --- Graphical SVG Tree layout ---

function renderGraph(info, queryPlan = null) {
  const svg = document.getElementById('graphSvg');
  const linksGroup = document.getElementById('linksGroup');
  const nodesGroup = document.getElementById('nodesGroup');

  linksGroup.innerHTML = '';
  nodesGroup.innerHTML = '';

  const width = svg.clientWidth || 600;
  const height = svg.clientHeight || 480;

  // 1. Build flat layout representation
  let leafCount = 0;
  
  function buildTree(node, level, parent = null) {
    let layoutNode = {
      value: node.value,
      stats: node.stats,
      physicalFiles: node.physical_files,
      virtualFileIds: node.virtual_file_ids,
      level: level,
      children: [],
      parent: parent
    };

    if (node.children && Object.keys(node.children).length > 0) {
      for (let key of Object.keys(node.children)) {
        layoutNode.children.push(buildTree(node.children[key], level + 1, layoutNode));
      }
    } else {
      leafCount++;
      layoutNode.leafIndex = leafCount;
    }
    return layoutNode;
  }

  // 2. Assign coordinates
  const rootLayout = buildTree(info.graph.root, 0);

  const totalLevels = info.graph_dimensions.length + 1;
  const ySpacing = (height - 80) / (totalLevels - 1 || 1);
  const xSpacing = (width - 100) / (leafCount || 1);

  // Position nodes
  const flatNodes = [];
  
  function positionNode(layoutNode) {
    layoutNode.y = layoutNode.level * ySpacing + 40;

    if (layoutNode.children.length === 0) {
      layoutNode.x = (layoutNode.leafIndex - 0.5) * xSpacing + 50;
    } else {
      layoutNode.children.forEach(positionNode);
      // parent is centered over children
      const childXSum = layoutNode.children.reduce((sum, c) => sum + c.x, 0);
      layoutNode.x = childXSum / layoutNode.children.length;
    }
    flatNodes.push(layoutNode);
  }
  
  positionNode(rootLayout);

  // 3. Render connections and circles
  flatNodes.forEach(node => {
    if (node.parent) {
      // Determine link state: Traversed / Pruned / Default
      let linkState = 'default';
      if (queryPlan) {
        const isParentTraversed = isPathTraversed(node.parent, queryPlan);
        const isChildTraversed = isPathTraversed(node, queryPlan);

        if (isParentTraversed && isChildTraversed) {
          linkState = 'traversed';
        } else if (isParentTraversed && !isChildTraversed) {
          linkState = 'pruned';
        }
      }

      const line = document.createElementNS('http://www.w3.org/2000/svg', 'line');
      line.setAttribute('x1', node.parent.x);
      line.setAttribute('y1', node.parent.y);
      line.setAttribute('x2', node.x);
      line.setAttribute('y2', node.y);
      line.setAttribute('class', `link ${linkState}`);
      linksGroup.appendChild(line);
    }
  });

  flatNodes.forEach(node => {
    // Determine node state
    let nodeState = 'default';
    if (queryPlan) {
      if (isPathTraversed(node, queryPlan)) {
        nodeState = 'traversed';
      } else if (node.parent && isPathTraversed(node.parent, queryPlan)) {
        nodeState = 'pruned';
      }
    }

    const g = document.createElementNS('http://www.w3.org/2000/svg', 'g');
    g.setAttribute('class', `node ${nodeState}`);
    g.style.cursor = 'pointer';

    // Hover tooltip info
    g.addEventListener('click', () => {
      const records = node.stats ? node.stats.row_count : 0;
      const files = node.stats ? node.stats.physical_file_count : 0;
      showToast(`Node: "${node.value}" (${records.toLocaleString()} rows, ${files} files)`);
    });

    const circle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    circle.setAttribute('cx', node.x);
    circle.setAttribute('cy', node.y);
    circle.setAttribute('r', 16);
    g.appendChild(circle);

    const txt = document.createElementNS('http://www.w3.org/2000/svg', 'text');
    txt.setAttribute('x', node.x);
    txt.setAttribute('y', node.y - 24);
    txt.setAttribute('class', 'node-val');
    txt.textContent = node.value === '__root__' ? 'ROOT' : node.value;
    g.appendChild(txt);

    if (node.stats && node.stats.row_count > 0 && node.value !== '__root__') {
      const subTxt = document.createElementNS('http://www.w3.org/2000/svg', 'text');
      subTxt.setAttribute('x', node.x);
      subTxt.setAttribute('y', node.y + 4);
      subTxt.style.fontSize = '9px';
      subTxt.style.fill = '#94a3b8';
      subTxt.textContent = formatCompactNum(node.stats.row_count);
      g.appendChild(subTxt);
    }

    nodesGroup.appendChild(g);
  });
}

function isPathTraversed(layoutNode, queryPlan) {
  // A node is traversed if the active query plan selected any virtual files
  // matching the node's lineage (i.e. child partition keys).
  // In our MVP, we can check if the activePredicates match this node's values!
  if (layoutNode.value === '__root__') return true;

  // Let's traverse up to collect values
  let curr = layoutNode;
  const values = [];
  while (curr && curr.value !== '__root__') {
    values.unshift(curr.value);
    curr = curr.parent;
  }

  // Match dimensions in order
  for (let i = 0; i < values.length; i++) {
    const dim = currentMetadata.graph_dimensions[i];
    const predicate = activePredicates[dim];
    
    if (predicate) {
      if (predicate.type === 'equal' && values[i] !== predicate.val) {
        return false;
      }
      if (predicate.type === 'range') {
        if (predicate.start && values[i] < predicate.start) return false;
        if (predicate.end && values[i] > predicate.end) return false;
      }
    }
  }
  return true;
}

// --- Event Handlers ---

async function handleCreateTableSubmit(e) {
  e.preventDefault();

  const name = document.getElementById('newTableName').value;
  const dimInputs = document.querySelectorAll('.dim-input');
  const graph_dims = Array.from(dimInputs).map(el => el.value);

  const fieldRows = document.querySelectorAll('#modalSchemaBody tr');
  const schema_fields = Array.from(fieldRows).map(row => {
    return {
      name: row.querySelector('.field-name').value,
      type: row.querySelector('.field-type').value,
      required: row.querySelector('.field-required').checked
    };
  });

  try {
    const response = await fetch(`${API_BASE}/table/create`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: jsonStringify({
        table_name: name,
        schema_fields,
        graph_dims
      })
    });
    
    if (!response.ok) {
      const err = await response.json();
      throw new Error(err.error || 'Failed to create table');
    }

    showToast(`Table "${name}" created successfully!`, 'success');
    document.getElementById('createTableModal').style.display = 'none';
    e.target.reset();
    
    // Refresh catalog
    await loadCatalogTree(name);
  } catch (e) {
    showToast(e.message, 'error');
  }
}

async function handleQuerySubmit(e) {
  e.preventDefault();
  if (!currentTable) return;

  const predicates = {};
  const ranges = {};

  currentMetadata.graph_dimensions.forEach(dim => {
    const eqVal = document.getElementById(`eq_${dim}`).value.trim();
    if (eqVal) {
      predicates[dim] = eqVal;
      activePredicates[dim] = { type: 'equal', val: eqVal };
    }

    const startVal = document.getElementById(`range_start_${dim}`).value.trim();
    const endVal = document.getElementById(`range_end_${dim}`).value.trim();
    if (startVal && endVal) {
      ranges[dim] = [startVal, endVal];
      activePredicates[dim] = { type: 'range', start: startVal, end: endVal };
    }
  });

  const snapshotInput = document.getElementById('querySnapshot').value;
  const snapshot = snapshotInput !== '' ? parseInt(snapshotInput, 10) : null;

  try {
    const response = await fetch(`${API_BASE}/table/${currentTable}/plan`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: jsonStringify({ predicates, ranges, snapshot })
    });

    if (!response.ok) {
      const err = await response.json();
      throw new Error(err.error || 'Planning failed');
    }

    const plan = await response.json();
    activeQueryPlan = plan;

    // Update visual graph
    renderGraph(currentMetadata, plan);

    // Update Diagnostics
    document.getElementById('valVisitedNodes').textContent = plan.visited_nodes;
    document.getElementById('valSkippedPhysical').textContent = plan.skipped_physical_files;
    document.getElementById('valPrunedNodes').textContent = plan.pruned_node_count;

    document.getElementById('valTotalPhysicalFilesScan').textContent = plan.manifest_scan_physical_files;
    document.getElementById('valPrunedSelectedFiles').textContent = plan.selected_physical_files;

    // Bar animation
    const prunePercent = plan.manifest_scan_physical_files > 0
      ? (plan.selected_physical_files / plan.manifest_scan_physical_files) * 100
      : 0;
    document.getElementById('barPrunedScan').style.width = `${prunePercent}%`;

    // Render Output Virtual Files
    const container = document.getElementById('planOutputContainer');
    container.innerHTML = '';
    
    if (plan.virtual_files.length === 0) {
      container.innerHTML = '<div class="empty-state">No matching files found.</div>';
      return;
    }

    plan.virtual_files.forEach(vf => {
      const item = document.createElement('div');
      item.className = 'vf-item';
      
      let filesList = '';
      vf.physical_files.forEach(file => {
        // Find if this file has a delete bitmap in the plan
        const deleteBitmap = plan.delete_bitmaps[file];
        const badge = deleteBitmap 
          ? `<span class="badge delete-badge" title="${deleteBitmap}">delete index applied</span>`
          : '';
        filesList += `
          <li class="vf-physical-item" style="padding: 6px 0;">
            <span class="file-path">${file}</span>
            ${badge}
          </li>
        `;
      });

      item.innerHTML = `
        <div class="vf-item-header">
          <span class="vf-id">${vf.id}</span>
          <span class="vf-records">${vf.record_count.toLocaleString()} rows</span>
        </div>
        <ul class="vf-physical-list">
          ${filesList}
        </ul>
      `;
      container.appendChild(item);
    });

  } catch (e) {
    showToast(e.message, 'error');
  }
}

async function handleAppendSubmit(e) {
  e.preventDefault();
  if (!currentTable) return;

  const file = document.getElementById('appendFileName').value.trim();
  const records = parseInt(document.getElementById('appendRecords').value, 10);
  const partitions = {};

  currentMetadata.graph_dimensions.forEach(dim => {
    partitions[dim] = document.getElementById(`append_part_${dim}`).value.trim();
  });

  try {
    const response = await fetch(`${API_BASE}/table/${currentTable}/append`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: jsonStringify({
        files: [file],
        records,
        partitions
      })
    });

    if (!response.ok) {
      const err = await response.json();
      throw new Error(err.error || 'Append failed');
    }

    showToast('Data file successfully appended and committed!', 'success');
    e.target.reset();
    
    // Refresh table details
    await selectTable(currentTable);
  } catch (e) {
    showToast(e.message, 'error');
  }
}

async function handleCompactSubmit(e) {
  e.preventDefault();
  if (!currentTable) return;

  const partitions = {};
  currentMetadata.graph_dimensions.forEach(dim => {
    partitions[dim] = document.getElementById(`compact_part_${dim}`).value.trim();
  });

  const target_file = document.getElementById('compactTargetFile').value.trim();

  try {
    const response = await fetch(`${API_BASE}/table/${currentTable}/compact`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: jsonStringify({ partitions, target_file })
    });

    if (!response.ok) {
      const err = await response.json();
      throw new Error(err.error || 'Compaction failed');
    }

    showToast('Compaction successfully completed and committed!', 'success');
    e.target.reset();

    await selectTable(currentTable);
  } catch (e) {
    showToast(e.message, 'error');
  }
}

async function handleDeleteSubmit(e) {
  e.preventDefault();
  if (!currentTable) return;

  const file = document.getElementById('deleteFilePath').value;
  const delete_bitmap = document.getElementById('deleteBitmapPath').value.trim();

  try {
    const response = await fetch(`${API_BASE}/table/${currentTable}/delete`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: jsonStringify({ file, delete_bitmap })
    });

    if (!response.ok) {
      const err = await response.json();
      throw new Error(err.error || 'Failed to apply deletes');
    }

    showToast('Delete bitmap successfully applied and committed!', 'success');
    e.target.reset();

    await selectTable(currentTable);
  } catch (e) {
    showToast(e.message, 'error');
  }
}

async function optimizeLayout(recommend) {
  if (!currentTable) return;
  const resultDiv = document.getElementById('optimizerResult');
  resultDiv.innerHTML = 'Analyzing query history...';

  try {
    const response = await fetch(`${API_BASE}/table/${currentTable}/optimize`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: jsonStringify({ recommend })
    });

    if (!response.ok) {
      const err = await response.json();
      throw new Error(err.error || 'Layout optimization failed');
    }

    const res = await response.json();
    resultDiv.textContent = res.message;

    showToast('Optimization layout check complete!', 'success');

    // Reload table to render reordered dimensions
    if (!recommend) {
      await selectTable(currentTable);
    }
  } catch (e) {
    resultDiv.textContent = `Error: ${e.message}`;
    showToast(e.message, 'error');
  }
}

// --- Utilities ---

function formatCompactNum(num) {
  if (num >= 1e6) return (num / 1e6).toFixed(1) + 'M';
  if (num >= 1e3) return (num / 1e3).toFixed(1) + 'k';
  return num.toString();
}

function jsonStringify(obj) {
  return JSON.stringify(obj);
}

function showToast(message, type = 'info') {
  const oldToast = document.querySelector('.toast');
  if (oldToast) oldToast.remove();

  const toast = document.createElement('div');
  toast.className = `toast ${type}`;
  toast.textContent = message;

  document.body.appendChild(toast);

  // Auto remove toast
  setTimeout(() => {
    toast.style.animation = 'slideIn 0.3s reverse';
    setTimeout(() => toast.remove(), 300);
  }, 4000);
}
