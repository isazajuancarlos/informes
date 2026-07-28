/**
 * Mis entregas.
 *
 * # Las dos fechas se MUESTRAN, no se resumen
 *
 * La pantalla anterior enseñaba una sola fecha, y por eso quien la miraba no
 * podía distinguir «vence el 1 de enero y hay plazo hasta el 3» de «vence el 3».
 * Aquí van las dos, con el ajuste explicado en el sitio: cuando el plazo se
 * movió, la columna lo dice y el globo explica por qué.
 *
 * # Lo que aporta el cliente, y que la versión servida por el servidor no daba
 *
 * Ordenar y filtrar sin volver al servidor, y una columna calculada —los días
 * que faltan— que colorea la urgencia. Son las tres cosas que un operador hace
 * cien veces al día y que antes costaban una recarga cada una.
 */
import { DatePipe } from '@angular/common';
import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatChipsModule } from '@angular/material/chips';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatTableModule } from '@angular/material/table';
import { MatTooltipModule } from '@angular/material/tooltip';
import { firstValueFrom } from 'rxjs';

import { Api, Entrega } from '../nucleo/api';

type Orden = 'fecha_limite' | 'informe' | 'estado';

@Component({
  selector: 'siger-entregas',
  imports: [
    DatePipe,
    FormsModule,
    MatButtonModule,
    MatCardModule,
    MatChipsModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressBarModule,
    MatTableModule,
    MatTooltipModule,
  ],
  template: `
    <header class="cabecera">
      <h2>Entregas</h2>
      <mat-form-field appearance="outline" subscriptSizing="dynamic">
        <mat-label>Filtrar</mat-label>
        <input matInput [(ngModel)]="filtro" (ngModelChange)="texto.set($event)"
               placeholder="informe o estado" />
        <mat-icon matSuffix>search</mat-icon>
      </mat-form-field>
    </header>

    @if (cargando()) { <mat-progress-bar mode="indeterminate" /> }

    @if (error()) {
      <mat-card class="aviso"><mat-icon>error_outline</mat-icon> {{ error() }}</mat-card>
    }

    @if (!cargando() && visibles().length === 0) {
      <mat-card class="vacio">
        <mat-icon>inbox</mat-icon>
        <p>
          @if (todas().length === 0) {
            No hay entregas que le correspondan todavía.
          } @else {
            Ninguna entrega coincide con «{{ texto() }}».
          }
        </p>
      </mat-card>
    } @else {
      <table mat-table [dataSource]="visibles()" class="tabla">
        <ng-container matColumnDef="informe">
          <th mat-header-cell *matHeaderCellDef (click)="ordenarPor('informe')">Informe</th>
          <td mat-cell *matCellDef="let e">
            {{ e.informe }}
            @if (e.tiene_firma) {
              <mat-icon class="firma" matTooltip="Entrega firmada: su contenido es verificable">
                verified
              </mat-icon>
            }
          </td>
        </ng-container>

        <ng-container matColumnDef="nominal">
          <th mat-header-cell *matHeaderCellDef
              matTooltip="El día que fija la norma o el contrato. No se mueve aunque sea festivo.">
            Vence
          </th>
          <td mat-cell *matCellDef="let e">{{ e.fecha_nominal | date: 'dd MMM yyyy' }}</td>
        </ng-container>

        <ng-container matColumnDef="limite">
          <th mat-header-cell *matHeaderCellDef (click)="ordenarPor('fecha_limite')"
              matTooltip="El día contra el que se mide el cumplimiento, ya resuelto el ajuste.">
            Plazo
          </th>
          <td mat-cell *matCellDef="let e">
            {{ e.fecha_limite | date: 'dd MMM yyyy' }}
            @if (e.tipo_ajuste !== 'NINGUNO') {
              <mat-icon class="ajuste" [matTooltip]="explicar(e.tipo_ajuste)">
                {{ e.tipo_ajuste === 'ANTICIPADO' ? 'fast_rewind' : 'fast_forward' }}
              </mat-icon>
            }
          </td>
        </ng-container>

        <ng-container matColumnDef="restan">
          <th mat-header-cell *matHeaderCellDef>Restan</th>
          <td mat-cell *matCellDef="let e" [class]="'urgencia ' + urgencia(e)">
            {{ textoRestante(e) }}
          </td>
        </ng-container>

        <ng-container matColumnDef="estado">
          <th mat-header-cell *matHeaderCellDef (click)="ordenarPor('estado')">Estado</th>
          <td mat-cell *matCellDef="let e">
            <mat-chip [class]="'estado ' + e.estado.toLowerCase()">{{ e.estado }}</mat-chip>
          </td>
        </ng-container>

        <ng-container matColumnDef="archivo">
          <th mat-header-cell *matHeaderCellDef></th>
          <td mat-cell *matCellDef="let e">
            @if (e.archivo_nombre) {
              <!-- La URL lleva el id, nunca la ruta: el servidor la construye y
                   comprueba antes que esta entrega es de quien la pide. -->
              <a mat-icon-button [href]="api.urlArchivo(e.id)" [matTooltip]="e.archivo_nombre"
                 [attr.aria-label]="'Descargar ' + e.archivo_nombre">
                <mat-icon>download</mat-icon>
              </a>
            }
          </td>
        </ng-container>

        <tr mat-header-row *matHeaderRowDef="columnas"></tr>
        <tr mat-row *matRowDef="let fila; columns: columnas"></tr>
      </table>
    }
  `,
  styles: `
    .cabecera { display: flex; align-items: center; gap: 1rem; justify-content: space-between; }
    .tabla { width: 100%; margin-top: 1rem; }
    th { cursor: pointer; user-select: none; }
    .vacio, .aviso {
      display: flex; align-items: center; gap: .75rem; padding: 2rem; margin-top: 1rem;
      justify-content: center; color: var(--mat-sys-on-surface-variant);
    }
    .firma { font-size: 1rem; height: 1rem; width: 1rem; vertical-align: middle;
             color: var(--mat-sys-primary); }
    .ajuste { font-size: 1rem; height: 1rem; width: 1rem; vertical-align: middle;
              color: var(--mat-sys-tertiary); }
    .urgencia.vencida { color: var(--mat-sys-error); font-weight: 600; }
    .urgencia.hoy     { color: var(--mat-sys-error); }
    .urgencia.pronto  { color: #b26a00; }
    .estado.cumplido      { --mdc-chip-label-text-color: #1b5e20; }
    .estado.extemporaneo  { --mdc-chip-label-text-color: #b26a00; }
    .estado.incumplido    { --mdc-chip-label-text-color: #b71c1c; }
  `,
})
export class Entregas {
  api = inject(Api);

  todas = signal<Entrega[]>([]);
  texto = signal('');
  filtro = '';
  cargando = signal(true);
  error = signal('');
  private orden = signal<Orden>('fecha_limite');

  readonly columnas = ['informe', 'nominal', 'limite', 'restan', 'estado', 'archivo'];

  readonly visibles = computed(() => {
    const t = this.texto().trim().toLowerCase();
    const filtradas = t
      ? this.todas().filter(
          (e) => e.informe.toLowerCase().includes(t) || e.estado.toLowerCase().includes(t),
        )
      : this.todas();
    const campo = this.orden();
    return [...filtradas].sort((a, b) => String(a[campo]).localeCompare(String(b[campo])));
  });

  constructor() {
    this.cargar();
  }

  async cargar() {
    this.cargando.set(true);
    try {
      this.todas.set(await firstValueFrom(this.api.entregas()));
      this.error.set('');
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }

  ordenarPor(campo: Orden) {
    this.orden.set(campo);
  }

  explicar(ajuste: Entrega['tipo_ajuste']) {
    switch (ajuste) {
      case 'ANTICIPADO':
        return 'El día nominal no es hábil: se entrega el hábil anterior. El vencimiento no cambia.';
      case 'GRACIA':
        return 'El día nominal no es hábil: hay plazo hasta el hábil siguiente.';
      case 'OMITIR':
        return 'Ese día era festivo y la obligación no se genera.';
      default:
        return '';
    }
  }

  private diasRestantes(e: Entrega) {
    const hoy = new Date();
    hoy.setHours(0, 0, 0, 0);
    const limite = new Date(e.fecha_limite + 'T00:00:00');
    return Math.round((limite.getTime() - hoy.getTime()) / 86_400_000);
  }

  textoRestante(e: Entrega) {
    if (e.estado !== 'PENDIENTE') return '—';
    const d = this.diasRestantes(e);
    if (d < 0) return `${-d} d de retraso`;
    if (d === 0) return 'hoy';
    return `${d} d`;
  }

  urgencia(e: Entrega) {
    if (e.estado !== 'PENDIENTE') return '';
    const d = this.diasRestantes(e);
    if (d < 0) return 'vencida';
    if (d === 0) return 'hoy';
    return d <= 3 ? 'pronto' : '';
  }
}
