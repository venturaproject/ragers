-- Seed schema for the "erp-db" container: a small, self-contained stand-in
-- for a real ERP/CRM system (products, customers, orders) that the RAG API
-- does NOT own and only reads from via ERP_DATABASE_URL. It exists purely to
-- exercise POST /api/rag/sync/erp with realistic structured data instead of
-- always relying on hand-uploaded PDFs.
--
-- Data simulates Mayor S.A. (wholesale office-supplies distributor, see the
-- company-profile PDF used in RAG testing) selling to a handful of business
-- customers.

CREATE TABLE customers (
    id    SERIAL PRIMARY KEY,
    name  TEXT NOT NULL,
    email TEXT NOT NULL,
    city  TEXT NOT NULL
);

CREATE TABLE products (
    id          SERIAL PRIMARY KEY,
    sku         TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    description TEXT NOT NULL,
    price       DOUBLE PRECISION NOT NULL
);

CREATE TABLE orders (
    id          SERIAL PRIMARY KEY,
    customer_id INTEGER NOT NULL REFERENCES customers(id),
    order_date  DATE NOT NULL,
    status      TEXT NOT NULL
);

CREATE TABLE order_items (
    id         SERIAL PRIMARY KEY,
    order_id   INTEGER NOT NULL REFERENCES orders(id),
    product_id INTEGER NOT NULL REFERENCES products(id),
    quantity   INTEGER NOT NULL,
    unit_price DOUBLE PRECISION NOT NULL
);

INSERT INTO customers (name, email, city) VALUES
    ('Distribuciones Norte SL',      'compras@distribucionesnorte.example', 'Alcobendas'),
    ('Oficinas Centrales Iberia SA', 'pedidos@oficentraliberia.example',    'Madrid'),
    ('Grupo Consultor Madrid SL',    'administracion@gcmadrid.example',     'Madrid'),
    ('Talleres Hermanos Ruiz SL',    'compras@tallereshruiz.example',       'Getafe');

INSERT INTO products (sku, name, description, price) VALUES
    ('OFC-1001', 'Silla ergonomica ejecutiva modelo Confort Pro',
     'Silla de oficina con reposabrazos ajustables, respaldo transpirable y soporte lumbar.', 189.90),
    ('OFC-1002', 'Mesa de reuniones 180cm',
     'Mesa rectangular de melamina para sala de reuniones, capacidad 6 personas.', 349.00),
    ('OFC-1003', 'Resma de papel A4 80g (500 hojas)',
     'Papel blanco multifuncion para impresora y fotocopiadora, caja de 5 resmas.', 24.50),
    ('OFC-1004', 'Toner compatible HP LaserJet 26A',
     'Cartucho de toner negro compatible, rendimiento aproximado 3100 paginas.', 42.75),
    ('OFC-1005', 'Armario metalico 2 puertas con llave',
     'Armario de acero para archivo, 4 estantes regulables, cierre con llave.', 215.00),
    ('OFC-1006', 'Pack 50 boligrafos azules punta fina',
     'Boligrafos de tinta azul, punta fina 0.7mm, caja de 50 unidades.', 18.20),
    ('OFC-1007', 'Monitor 24 pulgadas Full HD',
     'Monitor LED 24" resolucion 1920x1080, entrada HDMI y VGA.', 129.99),
    ('OFC-1008', 'Silla de oficina basica sin reposabrazos',
     'Silla giratoria de altura regulable, tapizado en tela gris.', 74.90);

INSERT INTO orders (customer_id, order_date, status) VALUES
    (1, '2026-07-03', 'entregado'),
    (2, '2026-07-15', 'entregado'),
    (2, '2026-08-02', 'en_transito'),
    (3, '2026-08-10', 'entregado'),
    (4, '2026-08-21', 'pendiente'),
    (1, '2026-09-01', 'entregado');

-- order 1: Distribuciones Norte SL
INSERT INTO order_items (order_id, product_id, quantity, unit_price) VALUES
    (1, 3, 20, 24.50),
    (1, 6, 10, 18.20);

-- order 2: Oficinas Centrales Iberia SA
INSERT INTO order_items (order_id, product_id, quantity, unit_price) VALUES
    (2, 1, 15, 189.90),
    (2, 2, 2, 349.00);

-- order 3: Oficinas Centrales Iberia SA (en transito)
INSERT INTO order_items (order_id, product_id, quantity, unit_price) VALUES
    (3, 7, 8, 129.99);

-- order 4: Grupo Consultor Madrid SL
INSERT INTO order_items (order_id, product_id, quantity, unit_price) VALUES
    (4, 5, 3, 215.00),
    (4, 4, 6, 42.75);

-- order 5: Talleres Hermanos Ruiz SL (pendiente)
INSERT INTO order_items (order_id, product_id, quantity, unit_price) VALUES
    (5, 8, 12, 74.90);

-- order 6: Distribuciones Norte SL (pedido grande, entregado)
INSERT INTO order_items (order_id, product_id, quantity, unit_price) VALUES
    (6, 1, 25, 189.90),
    (6, 5, 5, 215.00),
    (6, 3, 40, 24.50);
