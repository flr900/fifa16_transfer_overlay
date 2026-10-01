//! Parser minimalista da database FIFA t3db v8, adaptado do
//! `fifa16_db_parser.py` para uso in-process (sem alocação de JSON,
//! sem metadata XML — só o necessário para ler a tabela CZUM e
//! extrair alguns campos de um jogador específico por playerid).
//!
//! Mantido deliberadamente simples nesta fase (prova de conceito de
//! leitura). Não decodifica strings Huffman (nomes) ainda — só campos
//! inteiros (storage_type == 3), que é o que precisamos para
//! atributos/overall/potential.

#[derive(Debug, Clone)]
pub struct FieldDescriptor {
    pub short_name: [u8; 4],
    pub storage_type: u32,
    pub bit_offset: u32,
    pub depth: u32,
}

#[derive(Debug, Clone)]
pub struct TableDescriptor {
    pub short_name: [u8; 4],
    pub offset_abs: usize,
    pub record_size: u32,
    pub written_record_count: u16,
    pub field_count: u8,
    pub fields: Vec<FieldDescriptor>,
}

fn u32_at(data: &[u8], pos: usize) -> Option<u32> {
    data.get(pos..pos + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
}

fn u16_at(data: &[u8], pos: usize) -> Option<u16> {
    data.get(pos..pos + 2)
        .map(|s| u16::from_le_bytes(s.try_into().unwrap()))
}

fn u8_at(data: &[u8], pos: usize) -> Option<u8> {
    data.get(pos).copied()
}

/// Parseia o cabeçalho de uma database a partir do offset absoluto
/// (dentro do slice `data`) onde a assinatura "DB\0\x08..." começa.
/// Retorna a lista de tabelas (sem decodificar registros ainda).
pub fn parse_database_tables(data: &[u8], start: usize) -> Option<Vec<TableDescriptor>> {
    let table_count = u32_at(data, start + 16)? as usize;
    if table_count == 0 || table_count > 200 {
        // sanity check — uma database FIFA real tem dezenas de tabelas,
        // não centenas; isso filtra falsos positivos de assinatura.
        return None;
    }

    let directory_end = 24 + table_count * 8;
    let table_data_base = directory_end + 4;

    let mut directory: Vec<([u8; 4], u32)> = Vec::with_capacity(table_count);
    for i in 0..table_count {
        let p = start + 24 + i * 8;
        let short = data.get(p..p + 4)?;
        let mut short_arr = [0u8; 4];
        short_arr.copy_from_slice(short);
        let rel = u32_at(data, p + 4)?;
        directory.push((short_arr, rel));
    }

    let mut tables = Vec::with_capacity(table_count);

    for (short_name, rel) in directory {
        let off = start + table_data_base + rel as usize;

        let record_size = u32_at(data, off + 4)?;
        let written_record_count = u16_at(data, off + 18)?;
        let field_count = u8_at(data, off + 24)?;

        let mut fields = Vec::with_capacity(field_count as usize);
        for j in 0..field_count as usize {
            let p = off + 36 + j * 16;
            let storage_type = u32_at(data, p)?;
            let bit_offset = u32_at(data, p + 4)?;
            let field_short = data.get(p + 8..p + 12)?;
            let mut field_short_arr = [0u8; 4];
            field_short_arr.copy_from_slice(field_short);
            let depth = u32_at(data, p + 12)?;

            fields.push(FieldDescriptor {
                short_name: field_short_arr,
                storage_type,
                bit_offset,
                depth,
            });
        }

        tables.push(TableDescriptor {
            short_name,
            offset_abs: off,
            record_size,
            written_record_count,
            field_count,
            fields,
        });
    }

    Some(tables)
}

/// Lê um inteiro "packed" (bit-level) de um registro, espelhando
/// `fifa16_db_parser.read_packed_int`.
pub fn read_packed_int(record: &[u8], bit_offset: u32, depth: u32) -> Option<u32> {
    let shift = bit_offset % 8;
    let first = (bit_offset / 8) as usize;
    let count = ((shift + depth + 7) / 8) as usize;

    if first + count > record.len() {
        return None;
    }

    let mut packed: u64 = 0;
    for i in (0..count).rev() {
        packed = packed * 256 + record[first + i] as u64;
    }

    let mask = (1u64 << depth) - 1;
    Some(((packed >> shift) & mask) as u32)
}

/// Encontra o índice do registro cujo campo `playerid` bate com o
/// valor procurado, dentro da tabela CZUM. Retorna o índice do
/// registro (não o offset) para permitir reuso ao ler múltiplos
/// campos do mesmo jogador.
pub fn find_player_record_index(
    data: &[u8],
    table: &TableDescriptor,
    playerid_field: &FieldDescriptor,
    target_playerid: u32,
) -> Option<usize> {
    let records_start = table.offset_abs + 36 + table.field_count as usize * 16;

    for i in 0..table.written_record_count as usize {
        let rp = records_start + i * table.record_size as usize;
        let record = data.get(rp..rp + table.record_size as usize)?;
        let raw_pid = read_packed_int(record, playerid_field.bit_offset, playerid_field.depth)?;
        if raw_pid == target_playerid {
            return Some(i);
        }
    }

    None
}

/// Lê o valor de um campo específico para um registro (por índice)
/// já localizado.
pub fn read_field_for_record(
    data: &[u8],
    table: &TableDescriptor,
    field: &FieldDescriptor,
    record_index: usize,
) -> Option<u32> {
    let records_start = table.offset_abs + 36 + table.field_count as usize * 16;
    let rp = records_start + record_index * table.record_size as usize;
    let record = data.get(rp..rp + table.record_size as usize)?;
    read_packed_int(record, field.bit_offset, field.depth)
}

/// Calcula o endereço absoluto (dentro da memória do processo, não
/// relativo ao slice) e o offset de bits/tamanho em bytes necessários
/// para escrever um campo de um registro específico. Espelha a
/// lógica de `write_packed_int` do `fifa16_db_parser.py`: descobre
/// quantos bytes cobrem o campo (considerando que ele pode não estar
/// alinhado a byte) e retorna (endereco_do_primeiro_byte, count_bytes,
/// shift_bits) para o chamador montar o valor mascarado.
pub struct PackedFieldLocation {
    pub byte_offset_in_region: usize,
    pub byte_count: usize,
    pub shift: u32,
}

pub fn locate_packed_field(table: &TableDescriptor, field: &FieldDescriptor, record_index: usize) -> PackedFieldLocation {
    let records_start = table.offset_abs + 36 + table.field_count as usize * 16;
    let record_offset = records_start + record_index * table.record_size as usize;

    let shift = field.bit_offset % 8;
    let first = (field.bit_offset / 8) as usize;
    let count = ((shift + field.depth + 7) / 8) as usize;

    PackedFieldLocation {
        byte_offset_in_region: record_offset + first,
        byte_count: count,
        shift,
    }
}

/// Monta os novos bytes a escrever para um campo bit-packed,
/// preservando os bits vizinhos que não pertencem a este campo
/// (mesma lógica de `write_packed_int` do Python).
pub fn build_packed_bytes(
    current_bytes: &[u8],
    shift: u32,
    depth: u32,
    new_value: u32,
) -> Vec<u8> {
    let count = current_bytes.len();
    let mut packed: u64 = 0;
    for i in (0..count).rev() {
        packed = packed * 256 + current_bytes[i] as u64;
    }

    let mask: u64 = ((1u64 << depth) - 1) << shift;
    packed = (packed & !mask) | (((new_value as u64) << shift) & mask);

    let mut result = Vec::with_capacity(count);
    let mut p = packed;
    for _ in 0..count {
        result.push((p & 0xFF) as u8);
        p >>= 8;
    }
    result
}

pub fn field_by_shortname<'a>(
    table: &'a TableDescriptor,
    shortname: &str,
) -> Option<&'a FieldDescriptor> {
    let target = shortname.as_bytes();
    table
        .fields
        .iter()
        .find(|f| f.short_name.as_slice() == target)
}

pub fn shortname_str(bytes: &[u8; 4]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}
