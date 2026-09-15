//! Pulls records from an external ERP/CRM-style Postgres database
//! (`ERP_DATABASE_URL`, wired up in [`crate::bootstrap`]) and renders them as
//! plain-text "documents" ready for [`super::service::ingest_text`] — the
//! same chunk/embed/store pipeline `POST /api/rag/documents` uses for PDFs.
//! Demonstrates that the RAG index isn't limited to file uploads: a system
//! this API doesn't own (products/orders in this case) is just as valid a
//! knowledge source.

use std::collections::BTreeMap;

use sqlx::{PgPool, Row};

use crate::errors::{AppError, AppResult};

fn map_err(e: sqlx::Error) -> AppError {
    AppError::Internal(anyhow::anyhow!(e))
}

/// One ERP record rendered as ingestible text, plus a human-readable title
/// used as the `Document.filename`.
pub struct ErpDocument {
    pub title: String,
    pub text: String,
}

/// `products` → one document per SKU.
pub async fn fetch_products(pool: &PgPool) -> AppResult<Vec<ErpDocument>> {
    let rows = sqlx::query("SELECT sku, name, description, price FROM products ORDER BY sku")
        .fetch_all(pool)
        .await
        .map_err(map_err)?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let sku: String = row.try_get("sku").map_err(map_err)?;
        let name: String = row.try_get("name").map_err(map_err)?;
        let description: String = row.try_get("description").map_err(map_err)?;
        let price: f64 = row.try_get("price").map_err(map_err)?;

        let text = format!(
            "Ficha de producto - {name}\n\
             SKU: {sku}\n\
             Precio: USD {price:.2}\n\
             Descripcion: {description}"
        );
        out.push(ErpDocument {
            title: format!("erp-product-{sku}"),
            text,
        });
    }
    Ok(out)
}

/// One document listing every product with its price, sorted cheapest to
/// priciest, with the min/max called out explicitly. Superlative questions
/// ("¿cuál es el producto más caro?") need the whole catalog in one chunk —
/// spread across 8 separate per-product documents, plain top-k retrieval has
/// no guarantee of pulling all of them in, so it can't reliably answer
/// something like that even though the data is all indexed. This is a
/// deliberate, cheap workaround, not a substitute for a real aggregation
/// path (e.g. text-to-SQL) over a catalog too large to fit in one chunk.
pub async fn fetch_catalog_summary(pool: &PgPool) -> AppResult<Option<ErpDocument>> {
    let rows = sqlx::query("SELECT sku, name, price FROM products ORDER BY price ASC")
        .fetch_all(pool)
        .await
        .map_err(map_err)?;
    if rows.is_empty() {
        return Ok(None);
    }

    let mut lines = Vec::with_capacity(rows.len());
    let mut cheapest: Option<(String, f64)> = None;
    let mut priciest: Option<(String, f64)> = None;
    for row in &rows {
        let sku: String = row.try_get("sku").map_err(map_err)?;
        let name: String = row.try_get("name").map_err(map_err)?;
        let price: f64 = row.try_get("price").map_err(map_err)?;
        lines.push(format!("  - {name} (SKU {sku}): USD {price:.2}"));
        if cheapest.is_none() {
            cheapest = Some((name.clone(), price));
        }
        priciest = Some((name, price));
    }
    let (cheapest_name, cheapest_price) = cheapest.expect("rows non-empty");
    let (priciest_name, priciest_price) = priciest.expect("rows non-empty");

    let text = format!(
        "Catalogo completo de productos (ordenado de mas barato a mas caro)\n\
         Total de productos: {count}\n\
         Producto mas barato: {cheapest_name} - USD {cheapest_price:.2}\n\
         Producto mas caro: {priciest_name} - USD {priciest_price:.2}\n\n\
         Listado completo:\n{lines}",
        count = rows.len(),
        lines = lines.join("\n"),
    );

    Ok(Some(ErpDocument {
        title: "erp-catalogo-completo".to_string(),
        text,
    }))
}

/// `orders` joined with `customers` + `order_items` + `products` → one
/// document per order, its line items folded into the same text (an order
/// with 3 items is one coherent record, not 3 disconnected chunks of intro).
pub async fn fetch_orders(pool: &PgPool) -> AppResult<Vec<ErpDocument>> {
    let rows = sqlx::query(
        "SELECT o.id AS order_id, o.order_date, o.status, \
                c.name AS customer_name, c.city AS customer_city, \
                p.name AS product_name, oi.quantity, oi.unit_price \
         FROM orders o \
         JOIN customers c ON c.id = o.customer_id \
         JOIN order_items oi ON oi.order_id = o.id \
         JOIN products p ON p.id = oi.product_id \
         ORDER BY o.id, oi.id",
    )
    .fetch_all(pool)
    .await
    .map_err(map_err)?;

    struct OrderAcc {
        order_date: chrono::NaiveDate,
        status: String,
        customer_name: String,
        customer_city: String,
        lines: Vec<String>,
        total: f64,
    }

    let mut orders: BTreeMap<i32, OrderAcc> = BTreeMap::new();
    for row in rows {
        let order_id: i32 = row.try_get("order_id").map_err(map_err)?;
        let order_date: chrono::NaiveDate = row.try_get("order_date").map_err(map_err)?;
        let status: String = row.try_get("status").map_err(map_err)?;
        let customer_name: String = row.try_get("customer_name").map_err(map_err)?;
        let customer_city: String = row.try_get("customer_city").map_err(map_err)?;
        let quantity: i32 = row.try_get("quantity").map_err(map_err)?;
        let unit_price: f64 = row.try_get("unit_price").map_err(map_err)?;
        let product_name: String = row.try_get("product_name").map_err(map_err)?;
        let line_total = unit_price * quantity as f64;

        let acc = orders.entry(order_id).or_insert_with(|| OrderAcc {
            order_date,
            status,
            customer_name,
            customer_city,
            lines: Vec::new(),
            total: 0.0,
        });
        acc.lines.push(format!(
            "  - {quantity} x {product_name} (USD {unit_price:.2} c/u) = USD {line_total:.2}"
        ));
        acc.total += line_total;
    }

    let mut out = Vec::with_capacity(orders.len());
    for (order_id, acc) in orders {
        let text = format!(
            "Pedido #{order_id} - Estado: {status}\n\
             Cliente: {customer} ({city})\n\
             Fecha del pedido: {date}\n\
             Lineas:\n{lines}\n\
             Total del pedido: USD {total:.2}",
            status = acc.status,
            customer = acc.customer_name,
            city = acc.customer_city,
            date = acc.order_date,
            lines = acc.lines.join("\n"),
            total = acc.total,
        );
        out.push(ErpDocument {
            title: format!("erp-order-{order_id}"),
            text,
        });
    }
    Ok(out)
}
