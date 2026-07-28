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
import { Component, ElementRef, computed, inject, signal, viewChild } from '@angular/core';
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
            @if (e.archivo_valido === false) {
              <mat-icon class="revisar"
                        matTooltip="El contenido del archivo no concuerda con su extensión. Se registró, pero conviene revisarlo.">
                report_problem
              </mat-icon>
            }
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
            } @else if (e.estado === 'PENDIENTE' || e.estado === 'INCUMPLIDO') {
              <!-- El nombre del archivo NO viaja como destino: solo aporta la
                   extensión. El servidor construye el nombre desde el id. -->
              <button mat-stroked-button type="button" [disabled]="subiendo() === e.id"
                      (click)="elegir(e)">
                <mat-icon>upload</mat-icon>
                {{ subiendo() === e.id ? 'Enviando…' : 'Entregar' }}
              </button>
            }
          </td>
        </ng-container>

        <tr mat-header-row *matHeaderRowDef="columnas"></tr>
        <tr mat-row *matRowDef="let fila; columns: columnas"></tr>
      </table>
    }

    <!-- Un solo selector de archivo para toda la tabla: uno por fila multiplica
         los elementos ocultos por el número de entregas sin dar nada a cambio. -->
    <input #selector type="file" hidden (change)="subir($event)" />

    @if (aviso()) {
      <div class="aviso-flotante" [class.malo]="avisoMalo()" role="status">
        <mat-icon>{{ avisoMalo() ? 'error_outline' : 'check_circle' }}</mat-icon>
        {{ aviso() }}
      </div>
    }
  `,
  styles: `
    .cabecera { display: flex; align-items: center; gap: 1rem; justify-content: space-between; }
    .tabla { width: 100%; margin-top: 1rem; }
    .revisar { color: var(--mat-sys-tertiary); vertical-align: middle;
               margin-left: .35rem; font-size: 1.15rem; height: 1.15rem; width: 1.15rem; }
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
    .aviso-flotante {
      position: fixed; right: 1.5rem; bottom: 1.5rem; z-index: 10;
      display: flex; align-items: center; gap: .5rem;
      padding: .9rem 1.1rem; border-radius: .5rem; max-width: 28rem;
      background: var(--mat-sys-inverse-surface); color: var(--mat-sys-inverse-on-surface);
      box-shadow: 0 4px 16px rgb(0 0 0 / .3);
    }
    .aviso-flotante.malo { background: var(--mat-sys-error); color: var(--mat-sys-on-error); }
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

  readonly selector = viewChild.required<ElementRef<HTMLInputElement>>('selector');
  subiendo = signal<number | null>(null);
  aviso = signal('');
  avisoMalo = signal(false);
  private destino: Entrega | null = null;

  constructor() {
    this.cargar();
  }

  elegir(entrega: Entrega) {
    this.destino = entrega;
    const campo = this.selector().nativeElement;
    // Se limpia antes de abrir: sin esto, elegir dos veces el MISMO archivo no
    // dispara `change` la segunda vez y el botón parece no hacer nada.
    campo.value = '';
    campo.click();
  }

  async subir(evento: Event) {
    const archivo = (evento.target as HTMLInputElement).files?.[0];
    const entrega = this.destino;
    if (!archivo || !entrega) return;

    this.subiendo.set(entrega.id);
    try {
      const respuesta = await firstValueFrom(this.api.subirArchivo(entrega.id, archivo));
      // El estado lo decide el SERVIDOR comparando con la fecha límite. Suponer
      // «CUMPLIDO» aquí pintaría de verde una entrega que el servidor acaba de
      // marcar extemporánea.
      this.mostrar(
        respuesta.estado === 'EXTEMPORANEO'
          ? `Registrada FUERA DE PLAZO: ${entrega.informe}`
          : `Entrega registrada: ${entrega.informe}`,
        respuesta.estado === 'EXTEMPORANEO',
      );
      await this.cargar();
    } catch (e) {
      // El motivo viene del servidor —formato no admitido, tamaño— y se enseña
      // tal cual: un «no se pudo subir» genérico obliga a adivinar cuál de las
      // dos reglas se incumplió.
      this.mostrar((e as Error).message, true);
    } finally {
      this.subiendo.set(null);
      this.destino = null;
    }
  }

  private mostrar(texto: string, malo: boolean) {
    this.aviso.set(texto);
    this.avisoMalo.set(malo);
    setTimeout(() => this.aviso.set(''), malo ? 9000 : 4000);
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
